/// ClusterManager — Thread principal do cluster
/// Responsavel por:
/// 1. Escutar conexoes S2S de outros nos (Server Socket)
/// 2. Conectar ativamente aos peers listados na configuracao (Seed Nodes)
/// 3. Enviar heartbeats periodicos e detectar nos mortos
/// 4. Reconectar automaticamente a peers que cairam

use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use super::config::ClusterConfig;
use super::state::ClusterState;
use super::peer;
use super::envelope::{S2sEnvelope, S2sMessageType};
use crate::ws::hub::WsHub;

/// Gerenciador do Cluster. Criado e iniciado pelo Server quando enable_cluster() e chamado.
pub struct ClusterManager;

impl ClusterManager {
    /// Inicia o cluster manager em threads de background.
    /// Esta funcao retorna imediatamente; as threads rodam de forma autonoma.
    pub fn start(config: ClusterConfig, cluster_state: ClusterState, hub: WsHub) {
        let cfg = Arc::new(config);

        // Thread 1: Server Socket — escuta conexoes S2S de outros nos
        let listener_cfg = Arc::clone(&cfg);
        let listener_state = cluster_state.clone();
        let listener_hub = hub.clone();
        let _ = thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(move || {
                Self::s2s_listener_loop(listener_cfg, listener_state, listener_hub);
            });

        // Thread 2: Connector — conecta ativamente aos seed peers
        let connector_cfg = Arc::clone(&cfg);
        let connector_state = cluster_state.clone();
        let connector_hub = hub.clone();
        let _ = thread::Builder::new()
            .stack_size(64 * 1024)
            .spawn(move || {
                Self::connector_loop(connector_cfg, connector_state, connector_hub);
            });

        // Thread 3: Heartbeat + Dead peer detection
        let hb_cfg = Arc::clone(&cfg);
        let hb_state = cluster_state.clone();
        let _ = thread::Builder::new()
            .stack_size(32 * 1024)
            .spawn(move || {
                Self::heartbeat_loop(hb_cfg, hb_state);
            });
    }

    /// Loop que escuta conexoes TCP S2S de outros nos
    fn s2s_listener_loop(cfg: Arc<ClusterConfig>, state: ClusterState, hub: WsHub) {
        let bind_addr = format!("0.0.0.0:{}", cfg.s2s_port);
        let listener = match TcpListener::bind(&bind_addr) {
            Ok(l) => l,
            Err(_) => return,
        };

        for stream in listener.incoming() {
            match stream {
                Ok(mut tcp_stream) => {
                    // Le o handshake do peer (1 byte: node_id + 20 bytes HMAC)
                    let secret = cfg.cluster_secret.as_ref().map(|s| s.as_bytes());
                    if let Some(remote_id) = peer::read_handshake(&mut tcp_stream, secret) {
                        // Verifica se ja temos este peer conectado
                        let already_connected = {
                            let inner = state.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                            inner.peers.contains_key(&remote_id)
                        };

                        if !already_connected {
                            // Envia nosso node_id de volta (Handshake do Listener)
                            peer::send_handshake(&mut tcp_stream, cfg.node_id, secret);
                            
                            let sender = peer::spawn_peer_threads(
                                tcp_stream,
                                remote_id,
                                state.clone(),
                                hub.clone(),
                            );
                            state.register_peer(remote_id, sender);
                        }
                    }
                }
                Err(_) => {}
            }
        }
    }

    /// Loop que tenta conectar ativamente aos seed peers
    fn connector_loop(cfg: Arc<ClusterConfig>, state: ClusterState, hub: WsHub) {
        loop {
            for peer_addr in &cfg.peers {
                // Verifica se ja estamos conectados a este peer
                // (nao temos como saber o node_id a priori, entao tentamos conectar)
                if let Ok(mut stream) = TcpStream::connect(peer_addr) {
                    let secret = cfg.cluster_secret.as_ref().map(|s| s.as_bytes());
                    // Envia nosso node_id como handshake
                    if peer::send_handshake(&mut stream, cfg.node_id, secret) {
                        // Le o node_id do peer
                        if let Some(remote_id) = peer::read_handshake(&mut stream, secret) {
                            let already_connected = {
                                let inner = state.inner.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
                                inner.peers.contains_key(&remote_id)
                            };

                            if !already_connected {
                                let sender = peer::spawn_peer_threads(
                                    stream,
                                    remote_id,
                                    state.clone(),
                                    hub.clone(),
                                );
                                state.register_peer(remote_id, sender);
                            }
                        }
                    }
                }
            }

            // Espera antes de tentar reconectar
            thread::sleep(Duration::from_secs(5));
        }
    }

    /// Loop de heartbeat e deteccao de peers mortos
    fn heartbeat_loop(cfg: Arc<ClusterConfig>, state: ClusterState) {
        let timeout = cfg.heartbeat_interval_secs * cfg.heartbeat_missed_limit;

        loop {
            thread::sleep(Duration::from_secs(cfg.heartbeat_interval_secs));

            // Envia heartbeat para todos os peers
            let hb_envelope = S2sEnvelope {
                msg_type: S2sMessageType::Heartbeat,
                node_origin: cfg.node_id,
                message_seq: state.next_seq(),
                target: Vec::new(),
                payload: Vec::new(),
            };
            state.forward_to_all_peers(&hb_envelope);

            // Verifica peers mortos
            let dead_peers = state.check_dead_peers(timeout);
            for dead_id in dead_peers {
                state.unregister_peer(dead_id);
            }
        }
    }
}
