/// Estado compartilhado do Cluster
/// Contem a tabela de presenca, cache de deduplicacao e lideranca de salas.
/// Protegido por Mutex e compartilhado via Arc entre o Hub, o Manager e os Peers.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::time::SystemTime;
use super::envelope::S2sEnvelope;

/// Capacidade maxima do cache de deduplicacao antes de limpar entradas antigas
const DEDUP_CACHE_MAX: usize = 100_000;

/// Informacoes sobre um no vizinho (peer) do cluster
pub struct PeerInfo {
    /// Canal MPSC para enviar envelopes para a thread de escrita deste peer
    pub sender: mpsc::Sender<Arc<[u8]>>,
    /// Ultima vez que recebemos um heartbeat deste peer
    pub last_heartbeat: SystemTime,
    /// Se o peer esta considerado vivo
    pub alive: bool,
}

/// Estado interno do cluster (protegido por Mutex)
pub struct ClusterStateInner {
    /// Tabela de presenca: mapeia ID de usuario -> node_id onde ele esta conectado
    pub presence: HashMap<u64, u8>,

    /// Cache de deduplicacao: (node_origin, message_seq) -> timestamp
    pub seen_messages: HashMap<(u8, u64), SystemTime>,

    /// Mapa de peers conectados: node_id -> PeerInfo
    pub peers: HashMap<u8, PeerInfo>,

    /// Lideranca de salas: nome_sala -> node_id do lider
    pub room_leaders: HashMap<String, u8>,

    /// Salas que possuem presenca neste no (salas com pelo menos 1 cliente local)
    pub local_rooms: HashSet<String>,
}

/// Estado compartilhado do cluster (clone-friendly via Arc)
#[derive(Clone)]
pub struct ClusterState {
    pub inner: Arc<Mutex<ClusterStateInner>>,
    /// ID deste no
    pub node_id: u8,
    /// Contador sequencial atomico para mensagens originadas neste no
    pub message_counter: Arc<AtomicU64>,
}

impl ClusterState {
    /// Cria um novo estado de cluster vazio
    pub fn new(node_id: u8) -> Self {
        ClusterState {
            inner: Arc::new(Mutex::new(ClusterStateInner {
                presence: HashMap::new(),
                seen_messages: HashMap::new(),
                peers: HashMap::new(),
                room_leaders: HashMap::new(),
                local_rooms: HashSet::new(),
            })),
            node_id,
            message_counter: Arc::new(AtomicU64::new(1)),
        }
    }

    /// Gera o proximo numero sequencial para uma mensagem originada neste no
    pub fn next_seq(&self) -> u64 {
        self.message_counter.fetch_add(1, Ordering::Relaxed)
    }

    /// Verifica se uma mensagem ja foi vista (deduplicacao).
    /// Se nao foi vista, marca como vista e retorna true (mensagem nova).
    /// Se ja foi vista, retorna false (duplicata).
    pub fn check_and_mark(&self, node_origin: u8, message_seq: u64) -> bool {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let key = (node_origin, message_seq);

        if state.seen_messages.contains_key(&key) {
            return false; // Duplicata
        }

        // Limpa cache se estiver muito grande
        if state.seen_messages.len() >= DEDUP_CACHE_MAX {
            // Remove as entradas mais antigas (primeira metade)
            let mut entries: Vec<((u8, u64), SystemTime)> = 
                state.seen_messages.drain().collect();
            entries.sort_by_key(|(_, ts)| *ts);
            let keep_from = entries.len() / 2;
            state.seen_messages = entries[keep_from..].iter().cloned().collect();
        }

        state.seen_messages.insert(key, SystemTime::now());
        true // Mensagem nova
    }

    /// Registra a presenca de um usuario neste no
    pub fn register_presence(&self, user_id: u64) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.presence.insert(user_id, self.node_id);
    }

    /// Remove a presenca de um usuario
    pub fn unregister_presence(&self, user_id: u64) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.presence.remove(&user_id);
    }

    /// Registra presenca remota (usuario em outro no)
    pub fn register_remote_presence(&self, user_id: u64, remote_node_id: u8) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.presence.insert(user_id, remote_node_id);
    }

    /// Remove presenca remota
    pub fn unregister_remote_presence(&self, user_id: u64) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        // So remove se nao for local
        if let Some(&node) = state.presence.get(&user_id) {
            if node != self.node_id {
                state.presence.remove(&user_id);
            }
        }
    }

    /// Consulta em qual no um usuario esta conectado
    pub fn lookup_user_node(&self, user_id: u64) -> Option<u8> {
        let state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.presence.get(&user_id).copied()
    }

    /// Registra que uma sala tem presenca local neste no
    pub fn register_local_room(&self, room: &str) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.local_rooms.insert(room.to_string());
    }

    /// Registra ou atualiza o lider de uma sala
    pub fn set_room_leader(&self, room: &str, leader_node_id: u8) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.room_leaders.insert(room.to_string(), leader_node_id);
    }

    /// Consulta quem e o lider de uma sala
    pub fn get_room_leader(&self, room: &str) -> Option<u8> {
        let state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.room_leaders.get(room).copied()
    }

    /// Registra um novo peer conectado
    pub fn register_peer(&self, node_id: u8, sender: mpsc::Sender<Arc<[u8]>>) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.peers.insert(node_id, PeerInfo {
            sender,
            last_heartbeat: SystemTime::now(),
            alive: true,
        });
    }

    /// Remove um peer
    pub fn unregister_peer(&self, node_id: u8) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        state.peers.remove(&node_id);
    }

    /// Atualiza o timestamp do ultimo heartbeat de um peer
    pub fn update_peer_heartbeat(&self, node_id: u8) {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(peer) = state.peers.get_mut(&node_id) {
            peer.last_heartbeat = SystemTime::now();
            peer.alive = true;
        }
    }

    /// Envia um envelope codificado para todos os peers conectados
    pub fn forward_to_all_peers(&self, envelope: &S2sEnvelope) {
        let encoded = envelope.encode();
        let len_bytes = (encoded.len() as u32).to_be_bytes();
        let mut full_msg = Vec::with_capacity(4 + encoded.len());
        full_msg.extend_from_slice(&len_bytes);
        full_msg.extend_from_slice(&encoded);
        let shared: Arc<[u8]> = Arc::from(full_msg);

        let state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for (_, peer) in state.peers.iter() {
            if peer.alive {
                let _ = peer.sender.send(Arc::clone(&shared));
            }
        }
    }

    /// Envia um envelope codificado para todos os peers conectados, exceto um no especifico
    pub fn forward_to_all_peers_except(&self, envelope: &S2sEnvelope, exclude_node_id: u8) {
        let encoded = envelope.encode();
        let len_bytes = (encoded.len() as u32).to_be_bytes();
        let mut full_msg = Vec::with_capacity(4 + encoded.len());
        full_msg.extend_from_slice(&len_bytes);
        full_msg.extend_from_slice(&encoded);
        let shared: Arc<[u8]> = Arc::from(full_msg);

        let state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        for (&node_id, peer) in state.peers.iter() {
            if node_id != exclude_node_id && peer.alive {
                let _ = peer.sender.send(Arc::clone(&shared));
            }
        }
    }

    /// Envia um envelope codificado para um peer especifico
    pub fn forward_to_peer(&self, target_node_id: u8, envelope: &S2sEnvelope) -> bool {
        let encoded = envelope.encode();
        let len_bytes = (encoded.len() as u32).to_be_bytes();
        let mut full_msg = Vec::with_capacity(4 + encoded.len());
        full_msg.extend_from_slice(&len_bytes);
        full_msg.extend_from_slice(&encoded);
        let shared: Arc<[u8]> = Arc::from(full_msg);

        let state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if let Some(peer) = state.peers.get(&target_node_id) {
            if peer.alive {
                return peer.sender.send(shared).is_ok();
            }
        }
        false
    }

    /// Verifica peers mortos com base no timeout de heartbeat
    pub fn check_dead_peers(&self, timeout_secs: u64) -> Vec<u8> {
        let mut state = self.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut dead = Vec::new();

        for (&node_id, peer) in state.peers.iter_mut() {
            if let Ok(elapsed) = peer.last_heartbeat.elapsed() {
                if elapsed.as_secs() > timeout_secs {
                    peer.alive = false;
                    dead.push(node_id);
                }
            }
        }

        // Limpa presenca de usuarios dos nos mortos
        if !dead.is_empty() {
            state.presence.retain(|_, node| !dead.contains(node));
            
            // Reelege lideres de salas cujo lider morreu
            let rooms_to_reassign: Vec<String> = state.room_leaders
                .iter()
                .filter(|(_, &leader)| dead.contains(&leader))
                .map(|(room, _)| room.clone())
                .collect();

            for room in rooms_to_reassign {
                if state.local_rooms.contains(room.as_str()) {
                    state.room_leaders.insert(room, self.node_id);
                } else {
                    state.room_leaders.remove(&room);
                }
            }
        }

        dead
    }
}
