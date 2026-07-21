/// Gestao de conexoes TCP entre nos do cluster (Peers)
/// Cada peer possui uma thread de leitura e uma thread de escrita.

use std::net::TcpStream;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use std::io::Write;
use super::envelope::{self, S2sEnvelope, S2sMessageType};
use super::state::ClusterState;
use crate::ws::hub::WsHub;
/// Inicia as threads de leitura e escrita para um peer conectado.
/// Retorna o Sender para enviar envelopes para a thread de escrita deste peer.
pub fn spawn_peer_threads(
    stream: TcpStream,
    remote_node_id: u8,
    cluster_state: ClusterState,
    hub: WsHub,
) -> mpsc::Sender<Arc<[u8]>> {
    let (tx, rx) = mpsc::channel::<Arc<[u8]>>();

    // Thread de escrita: le do MPSC e escreve no TCP do peer
    let mut write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return tx, // Falha de SO (FD esgotado), descarta sem panic
    };
    let _ = thread::Builder::new()
        .stack_size(32 * 1024)
        .spawn(move || {
            for data in rx {
                if write_stream.write_all(&data).is_err() {
                    break;
                }
            }
        });

    // Thread de leitura: le envelopes do TCP e processa
    let read_cluster = cluster_state.clone();
    let read_hub = hub.clone();
    let mut read_stream = stream;
    let _ = thread::Builder::new()
        .stack_size(64 * 1024)
        .spawn(move || {
            loop {
                match envelope::read_envelope(&mut read_stream) {
                    Some(env) => {
                        process_incoming_envelope(env, remote_node_id, &read_cluster, &read_hub);
                    }
                    None => {
                        // Conexao com o peer caiu
                        read_cluster.unregister_peer(remote_node_id);
                        break;
                    }
                }
            }
        });

    tx
}

/// Processa um envelope recebido de um peer remoto
fn process_incoming_envelope(
    env: S2sEnvelope,
    from_node_id: u8,
    cluster_state: &ClusterState,
    hub: &WsHub,
) {
    // Deduplicacao: verifica se ja vimos esta mensagem
    if !cluster_state.check_and_mark(env.node_origin, env.message_seq) {
        return; // Duplicata, descarta
    }

    match env.msg_type {
        S2sMessageType::Heartbeat => {
            cluster_state.update_peer_heartbeat(env.node_origin);
        }

        S2sMessageType::Broadcast => {
            hub.broadcast_local_raw(&env.payload);
            cluster_state.forward_to_all_peers_except(&env, from_node_id);
        }

        S2sMessageType::BroadcastExcept => {
            if env.target.len() >= 8 {
                let exclude_id = u64::from_be_bytes([
                    env.target[0], env.target[1], env.target[2], env.target[3],
                    env.target[4], env.target[5], env.target[6], env.target[7],
                ]);
                hub.broadcast_except_local_raw(exclude_id, &env.payload);
                cluster_state.forward_to_all_peers_except(&env, from_node_id);
            }
        }

        S2sMessageType::BroadcastRoom => {
            let room = String::from_utf8_lossy(&env.target).to_string();
            hub.broadcast_to_room_local_raw(&room, &env.payload);
            cluster_state.forward_to_all_peers_except(&env, from_node_id);
        }

        S2sMessageType::BroadcastRoomExcept => {
            if env.target.len() > 8 {
                let exclude_id = u64::from_be_bytes([
                    env.target[0], env.target[1], env.target[2], env.target[3],
                    env.target[4], env.target[5], env.target[6], env.target[7],
                ]);
                let room = String::from_utf8_lossy(&env.target[8..]).to_string();
                hub.broadcast_to_room_except_local_raw(&room, exclude_id, &env.payload);
                cluster_state.forward_to_all_peers_except(&env, from_node_id);
            }
        }

        S2sMessageType::SendTo => {
            if env.target.len() >= 8 {
                let target_id = u64::from_be_bytes([
                    env.target[0], env.target[1], env.target[2], env.target[3],
                    env.target[4], env.target[5], env.target[6], env.target[7],
                ]);
                
                // Se o usuario esta conectado localmente, enviamos para ele.
                let mut is_local = false;
                if let Some(node) = cluster_state.lookup_user_node(target_id) {
                    if node == cluster_state.node_id {
                        is_local = true;
                    }
                }
                
                if is_local {
                    hub.send_to_local_raw(target_id, &env.payload);
                }
                
                // Retransmite a mensagem para a malha (gossip), pois ele pode nao estar 
                // neste no, ou a tabela de presenca pode estar levemente dessincronizada
                cluster_state.forward_to_all_peers_except(&env, from_node_id);
            }
        }

        S2sMessageType::PresenceUpdate => {
            if env.payload.len() >= 9 {
                let action = env.payload[0];
                let user_id = u64::from_be_bytes([
                    env.payload[1], env.payload[2], env.payload[3], env.payload[4],
                    env.payload[5], env.payload[6], env.payload[7], env.payload[8],
                ]);
                if action == 1 {
                    cluster_state.register_remote_presence(user_id, env.node_origin);
                } else {
                    cluster_state.unregister_remote_presence(user_id);
                }
                
                cluster_state.forward_to_all_peers_except(&env, from_node_id);
            }
        }
    }
}

/// Realiza o handshake inicial entre dois nos do cluster.
/// O no que inicia a conexao envia seu node_id como primeiro byte.
/// Retorna o node_id do peer remoto.
pub fn send_handshake(stream: &mut TcpStream, my_node_id: u8) -> bool {
    stream.write_all(&[my_node_id]).is_ok()
}

/// Le o handshake do peer (1 byte: node_id)
pub fn read_handshake(stream: &mut TcpStream) -> Option<u8> {
    let mut buf = [0u8; 1];
    use std::io::Read;
    if stream.read_exact(&mut buf).is_ok() {
        Some(buf[0])
    } else {
        None
    }
}
