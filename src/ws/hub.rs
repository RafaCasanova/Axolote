/// WsHub — Hub Central para Broadcast, Rooms e Mensagens Diretas
/// Compartilhado entre todas as threads de conexão WebSocket via Arc
/// Utiliza sharding (array de Mutexes) para evitar lock contention.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{SystemTime, Duration};
use super::frame::{self, Opcode};
use super::cluster::{ClusterState, envelope::{S2sEnvelope, S2sMessageType}};
use crate::json::ToJson;

const SHARD_COUNT: usize = 16;

/// Contador global de IDs de conexão (thread-safe, sem Mutex)
static NEXT_ID: AtomicU64 = AtomicU64::new(1);

/// Gera o próximo ID único de conexão
pub fn next_connection_id() -> u64 {
    NEXT_ID.fetch_add(1, Ordering::Relaxed)
}

/// Dados internos de um cliente registrado no Hub
struct HubClient {
    sender: mpsc::Sender<Arc<[u8]>>,
    rooms: HashSet<String>,
    metadata: HashMap<String, String>,
    last_pong: Arc<Mutex<SystemTime>>,
    last_ping: SystemTime,
    ping_interval: Option<u64>,
    pong_timeout: Option<u64>,
}

/// Estrutura interna do Hub para um shard
struct HubInner {
    clients: HashMap<u64, HubClient>,
}

/// Hub Central — Clone-friendly via Arc
#[derive(Clone)]
pub struct WsHub {
    shards: Arc<Vec<Mutex<HubInner>>>,
    pub(crate) cluster_state: Option<ClusterState>, // NEW
}

impl WsHub {
    /// Cria um novo Hub vazio e inicia a thread global de timer
    pub fn new() -> Self {
        let mut shards = Vec::with_capacity(SHARD_COUNT);
        for _ in 0..SHARD_COUNT {
            shards.push(Mutex::new(HubInner {
                clients: HashMap::new(),
            }));
        }
        
        let hub = WsHub {
            shards: Arc::new(shards),
            cluster_state: None,
        };

        // Inicia a thread global de timer (Feature 7 e Feature 6)
        let timer_hub = hub.clone();
        let _ = thread::Builder::new()
            .stack_size(32 * 1024)
            .spawn(move || {
                timer_hub.timer_loop();
            });

        hub
    }

    /// Injeta o ClusterState para habilitar o modo distribuido
    pub(crate) fn enable_cluster(&mut self, state: ClusterState) {
        self.cluster_state = Some(state);
    }

    /// Loop da thread global de timer
    fn timer_loop(&self) {
        let ping_frame = frame::encode_frame(Opcode::Ping, b"heartbeat");
        let shared_ping: Arc<[u8]> = Arc::from(ping_frame);

        loop {
            thread::sleep(Duration::from_secs(1));

            let mut expired_ids = Vec::new();

            for shard_mutex in self.shards.iter() {
                let mut shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                for (&id, client) in shard.clients.iter_mut() {
                    if let (Some(interval), Some(timeout)) = (client.ping_interval, client.pong_timeout) {
                        let now = SystemTime::now();
                        
                        // Verifica timeout
                        if let Ok(elapsed_since_pong) = client.last_pong.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).elapsed() {
                            if elapsed_since_pong.as_secs() > (interval + timeout) {
                                expired_ids.push(id);
                                continue;
                            }
                        }

                        // Verifica se precisa enviar novo ping
                        if let Ok(elapsed_since_ping) = client.last_ping.elapsed() {
                            if elapsed_since_ping.as_secs() >= interval {
                                let _ = client.sender.send(Arc::clone(&shared_ping));
                                client.last_ping = now;
                            }
                        }
                    }
                }
            }

            // Chutar os expirados
            for id in expired_ids {
                self.kick(id);
                self.unregister(id);
            }
        }
    }

    /// Retorna o Mutex guard do shard correspondente a um ID
    fn get_shard<'a>(&'a self, id: u64) -> std::sync::MutexGuard<'a, HubInner> {
        self.shards[id as usize % SHARD_COUNT].lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Registra um novo cliente no Hub
    pub(crate) fn register(
        &self, 
        id: u64, 
        sender: mpsc::Sender<Arc<[u8]>>, 
        last_pong: Arc<Mutex<SystemTime>>,
        ping_interval: Option<u64>,
        pong_timeout: Option<u64>,
    ) {
        {
            let mut shard = self.get_shard(id);
            shard.clients.insert(id, HubClient {
                sender,
                rooms: HashSet::new(),
                metadata: HashMap::new(),
                last_pong,
                last_ping: SystemTime::now(),
                ping_interval,
                pong_timeout,
            });
        }
        
        // Atualiza presenca no cluster
        if let Some(state) = &self.cluster_state {
            state.register_presence(id);
            let mut payload = vec![1u8]; // 1 = connect
            payload.extend_from_slice(&id.to_be_bytes());
            let env = S2sEnvelope {
                msg_type: S2sMessageType::PresenceUpdate,
                node_origin: state.node_id,
                message_seq: state.next_seq(),
                target: Vec::new(),
                payload,
            };
            state.forward_to_all_peers(&env);
        }
    }

    /// Remove um cliente do Hub
    pub(crate) fn unregister(&self, id: u64) {
        {
            let mut shard = self.get_shard(id);
            shard.clients.remove(&id);
        }
        
        // Atualiza presenca no cluster
        if let Some(state) = &self.cluster_state {
            state.unregister_presence(id);
            let mut payload = vec![0u8]; // 0 = disconnect
            payload.extend_from_slice(&id.to_be_bytes());
            let env = S2sEnvelope {
                msg_type: S2sMessageType::PresenceUpdate,
                node_origin: state.node_id,
                message_seq: state.next_seq(),
                target: Vec::new(),
                payload,
            };
            state.forward_to_all_peers(&env);
        }
    }

    /// Verifica se um cliente com este ID já está conectado (localmente ou no cluster)
    pub fn is_connected(&self, id: u64) -> bool {
        if let Some(state) = &self.cluster_state {
            state.lookup_user_node(id).is_some()
        } else {
            let shard = self.get_shard(id);
            shard.clients.contains_key(&id)
        }
    }

    /// Tenta alterar o ID de um cliente
    pub(crate) fn change_client_id(&self, old_id: u64, new_id: u64) -> bool {
        let old_idx = (old_id as usize) % SHARD_COUNT;
        let new_idx = (new_id as usize) % SHARD_COUNT;

        let success = if old_idx == new_idx {
            let mut shard = self.shards[old_idx].lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            if shard.clients.contains_key(&new_id) {
                false
            } else if let Some(client) = shard.clients.remove(&old_id) {
                shard.clients.insert(new_id, client);
                true
            } else {
                false
            }
        } else {
            let mut first_lock;
            let mut second_lock;
            
            if old_idx < new_idx {
                first_lock = self.shards[old_idx].lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                second_lock = self.shards[new_idx].lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            } else {
                first_lock = self.shards[new_idx].lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                second_lock = self.shards[old_idx].lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            }

            let (old_shard, new_shard) = if old_idx < new_idx {
                (&mut *first_lock, &mut *second_lock)
            } else {
                (&mut *second_lock, &mut *first_lock)
            };

            if new_shard.clients.contains_key(&new_id) {
                false
            } else if let Some(client) = old_shard.clients.remove(&old_id) {
                new_shard.clients.insert(new_id, client);
                true
            } else {
                false
            }
        };

        if success {
            if let Some(state) = &self.cluster_state {
                // Remove old, add new
                state.unregister_presence(old_id);
                state.register_presence(new_id);

                let mut payload_old = vec![0u8];
                payload_old.extend_from_slice(&old_id.to_be_bytes());
                state.forward_to_all_peers(&S2sEnvelope {
                    msg_type: S2sMessageType::PresenceUpdate,
                    node_origin: state.node_id,
                    message_seq: state.next_seq(),
                    target: Vec::new(),
                    payload: payload_old,
                });

                let mut payload_new = vec![1u8];
                payload_new.extend_from_slice(&new_id.to_be_bytes());
                state.forward_to_all_peers(&S2sEnvelope {
                    msg_type: S2sMessageType::PresenceUpdate,
                    node_origin: state.node_id,
                    message_seq: state.next_seq(),
                    target: Vec::new(),
                    payload: payload_new,
                });
            }
        }

        success
    }

    // ========================================================================
    // METODOS DE ENVIO PUBLICOS (Acionam o cluster se ativado)
    // ========================================================================

    /// Envia uma mensagem de texto para TODOS os clientes conectados
    pub fn broadcast(&self, msg: &str) {
        let frame_bytes = frame::encode_frame(Opcode::Text, msg.as_bytes());
        self.broadcast_local_raw(&frame_bytes);

        if let Some(state) = &self.cluster_state {
            let env = S2sEnvelope {
                msg_type: S2sMessageType::Broadcast,
                node_origin: state.node_id,
                message_seq: state.next_seq(),
                target: Vec::new(),
                payload: frame_bytes,
            };
            state.forward_to_all_peers(&env);
        }
    }

    /// Envia uma mensagem de texto para TODOS os clientes conectados, EXCETO o exclude_id
    pub fn broadcast_except(&self, exclude_id: u64, msg: &str) {
        let frame_bytes = frame::encode_frame(Opcode::Text, msg.as_bytes());
        self.broadcast_except_local_raw(exclude_id, &frame_bytes);

        if let Some(state) = &self.cluster_state {
            let mut target = Vec::new();
            target.extend_from_slice(&exclude_id.to_be_bytes());
            let env = S2sEnvelope {
                msg_type: S2sMessageType::BroadcastExcept,
                node_origin: state.node_id,
                message_seq: state.next_seq(),
                target,
                payload: frame_bytes,
            };
            state.forward_to_all_peers(&env);
        }
    }

    /// Envia uma mensagem para todos os clientes que estão numa sala específica
    pub fn broadcast_to_room(&self, room: &str, msg: &str) {
        let frame_bytes = frame::encode_frame(Opcode::Text, msg.as_bytes());
        self.broadcast_to_room_local_raw(room, &frame_bytes);

        if let Some(state) = &self.cluster_state {
            let env = S2sEnvelope {
                msg_type: S2sMessageType::BroadcastRoom,
                node_origin: state.node_id,
                message_seq: state.next_seq(),
                target: room.as_bytes().to_vec(),
                payload: frame_bytes,
            };
            state.forward_to_all_peers(&env);
        }
    }

    /// Envia uma mensagem para todos numa sala, exceto um ID específico
    pub fn broadcast_to_room_except(&self, room: &str, exclude_id: u64, msg: &str) {
        let frame_bytes = frame::encode_frame(Opcode::Text, msg.as_bytes());
        self.broadcast_to_room_except_local_raw(room, exclude_id, &frame_bytes);

        if let Some(state) = &self.cluster_state {
            let mut target = Vec::new();
            target.extend_from_slice(&exclude_id.to_be_bytes());
            target.extend_from_slice(room.as_bytes());
            let env = S2sEnvelope {
                msg_type: S2sMessageType::BroadcastRoomExcept,
                node_origin: state.node_id,
                message_seq: state.next_seq(),
                target,
                payload: frame_bytes,
            };
            state.forward_to_all_peers(&env);
        }
    }

    /// Envia uma mensagem para um cliente específico pelo ID
    pub fn send_to(&self, id: u64, msg: &str) -> bool {
        let frame_bytes = frame::encode_frame(Opcode::Text, msg.as_bytes());
        
        // Tenta enviar localmente primeiro
        if self.send_to_local_raw(id, &frame_bytes) {
            return true;
        }

        // Se falhou e o cluster esta ativo, encaminha
        if let Some(state) = &self.cluster_state {
            if let Some(remote_node_id) = state.lookup_user_node(id) {
                let mut target = Vec::new();
                target.extend_from_slice(&id.to_be_bytes());
                let env = S2sEnvelope {
                    msg_type: S2sMessageType::SendTo,
                    node_origin: state.node_id,
                    message_seq: state.next_seq(),
                    target,
                    payload: frame_bytes,
                };
                return state.forward_to_peer(remote_node_id, &env);
            }
        }
        
        
        false
    }

    /// Envia um objeto JSON para todos os clientes conectados
    pub fn broadcast_json<T: ToJson>(&self, data: &T) {
        self.broadcast(&data.to_json());
    }

    /// Envia um objeto JSON para todos os clientes, exceto um ID específico
    pub fn broadcast_json_except<T: ToJson>(&self, exclude_id: u64, data: &T) {
        self.broadcast_except(exclude_id, &data.to_json());
    }

    /// Envia um objeto JSON para todos os clientes numa sala
    pub fn broadcast_json_to_room<T: ToJson>(&self, room: &str, data: &T) {
        self.broadcast_to_room(room, &data.to_json());
    }

    /// Envia um objeto JSON para todos numa sala, exceto um ID
    pub fn broadcast_json_to_room_except<T: ToJson>(&self, room: &str, exclude_id: u64, data: &T) {
        self.broadcast_to_room_except(room, exclude_id, &data.to_json());
    }

    /// Envia um objeto JSON para um cliente específico
    pub fn send_json_to<T: ToJson>(&self, id: u64, data: &T) -> bool {
        self.send_to(id, &data.to_json())
    }

    // ========================================================================
    // METODOS RAW LOCAIS (Usados pelo Peer do cluster)
    // ========================================================================

    pub(crate) fn broadcast_local_raw(&self, frame_bytes: &[u8]) {
        let shared_frame: Arc<[u8]> = Arc::from(frame_bytes);
        for shard_mutex in self.shards.iter() {
            let shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for client in shard.clients.values() {
                let _ = client.sender.send(Arc::clone(&shared_frame));
            }
        }
    }

    pub(crate) fn broadcast_except_local_raw(&self, exclude_id: u64, frame_bytes: &[u8]) {
        let shared_frame: Arc<[u8]> = Arc::from(frame_bytes);
        for shard_mutex in self.shards.iter() {
            let shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for (id, client) in shard.clients.iter() {
                if *id != exclude_id {
                    let _ = client.sender.send(Arc::clone(&shared_frame));
                }
            }
        }
    }

    pub(crate) fn broadcast_to_room_local_raw(&self, room: &str, frame_bytes: &[u8]) {
        let shared_frame: Arc<[u8]> = Arc::from(frame_bytes);
        for shard_mutex in self.shards.iter() {
            let shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for client in shard.clients.values() {
                if client.rooms.contains(room) {
                    let _ = client.sender.send(Arc::clone(&shared_frame));
                }
            }
        }
    }

    pub(crate) fn broadcast_to_room_except_local_raw(&self, room: &str, exclude_id: u64, frame_bytes: &[u8]) {
        let shared_frame: Arc<[u8]> = Arc::from(frame_bytes);
        for shard_mutex in self.shards.iter() {
            let shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            for (id, client) in shard.clients.iter() {
                if *id != exclude_id && client.rooms.contains(room) {
                    let _ = client.sender.send(Arc::clone(&shared_frame));
                }
            }
        }
    }

    pub(crate) fn send_to_local_raw(&self, id: u64, frame_bytes: &[u8]) -> bool {
        let shared_frame: Arc<[u8]> = Arc::from(frame_bytes);
        let shard = self.get_shard(id);
        if let Some(client) = shard.clients.get(&id) {
            client.sender.send(shared_frame).is_ok()
        } else {
            false
        }
    }

    // ========================================================================
    // OUTROS METODOS
    // ========================================================================

    /// Fecha a conexão de um cliente específico (kick)
    pub fn kick(&self, id: u64) {
        let close_bytes = frame::encode_frame(Opcode::Close, &1000u16.to_be_bytes());
        let shared_frame: Arc<[u8]> = Arc::from(close_bytes);

        let shard = self.get_shard(id);
        if let Some(client) = shard.clients.get(&id) {
            let _ = client.sender.send(shared_frame);
        }
    }

    /// Adiciona um cliente a uma sala
    pub fn join_room(&self, id: u64, room: &str) {
        {
            let mut shard = self.get_shard(id);
            if let Some(client) = shard.clients.get_mut(&id) {
                client.rooms.insert(room.to_string());
            }
        }
        if let Some(state) = &self.cluster_state {
            state.register_local_room(room);
            // Definimos esse no como lider se nao houver nenhum
            if state.get_room_leader(room).is_none() {
                state.set_room_leader(room, state.node_id);
            }
        }
    }

    /// Remove um cliente de uma sala
    pub fn leave_room(&self, id: u64, room: &str) {
        let mut shard = self.get_shard(id);
        if let Some(client) = shard.clients.get_mut(&id) {
            client.rooms.remove(room);
        }
    }

    /// Define um metadado para um cliente
    pub fn set_client_metadata(&self, id: u64, key: &str, value: &str) {
        let mut shard = self.get_shard(id);
        if let Some(client) = shard.clients.get_mut(&id) {
            client.metadata.insert(key.to_string(), value.to_string());
        }
    }

    /// Lê um metadado de um cliente
    pub fn get_client_metadata(&self, id: u64, key: &str) -> Option<String> {
        let shard = self.get_shard(id);
        shard.clients.get(&id).and_then(|c| c.metadata.get(key).cloned())
    }

    /// Retorna quantos clientes estão conectados localmente
    pub fn count(&self) -> usize {
        let mut total = 0;
        for shard_mutex in self.shards.iter() {
            let shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            total += shard.clients.len();
        }
        total
    }

    /// Retorna quantos clientes estão numa sala localmente
    pub fn room_count(&self, room: &str) -> usize {
        let mut total = 0;
        for shard_mutex in self.shards.iter() {
            let shard = shard_mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
            total += shard.clients.values().filter(|c| c.rooms.contains(room)).count();
        }
        total
    }
}
