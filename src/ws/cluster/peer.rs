use super::envelope::{S2sEnvelope, S2sMessageType};
use super::state::ClusterState;
use crate::ws::hub::WsHub;
use std::io::Read;
use std::io::Write;
/// Gestao de conexoes TCP entre nos do cluster (Peers)
/// Cada peer possui uma thread de leitura e uma thread de escrita.
use std::net::TcpStream;
use std::os::unix::io::AsRawFd;
use std::sync::mpsc;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;

struct S2sConnectionState {
    stream: TcpStream,
    raw_data: Vec<u8>,
}

enum S2sEventAction {
    KeepAlive,
    Close,
}

fn process_s2s_event(
    state_mutex: &Mutex<S2sConnectionState>,
    cluster_state: &ClusterState,
    hub: &WsHub,
    remote_node_id: u8,
) -> S2sEventAction {
    let mut state = state_mutex.lock().unwrap();
    let mut buffer = [0; 4096];

    // 1. Ler tudo que está no socket
    loop {
        match state.stream.read(&mut buffer) {
            Ok(0) => return S2sEventAction::Close,
            Ok(n) => state.raw_data.extend_from_slice(&buffer[..n]),
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                break
            }
            Err(_) => return S2sEventAction::Close,
        }
    }

    // 2. Processa os envelopes acumulados
    loop {
        if state.raw_data.len() < 4 {
            return S2sEventAction::KeepAlive;
        }

        let mut len_buf = [0u8; 4];
        len_buf.copy_from_slice(&state.raw_data[0..4]);
        let total_len = u32::from_be_bytes(len_buf) as usize;

        if total_len > 1_048_576 {
            return S2sEventAction::Close;
        }

        if state.raw_data.len() < 4 + total_len {
            return S2sEventAction::KeepAlive;
        }

        let payload = &state.raw_data[4..4 + total_len];
        let secret = cluster_state.cluster_secret.as_ref().map(|s| s.as_slice());

        if let Some(env) = S2sEnvelope::decode(payload, secret) {
            process_incoming_envelope(env, remote_node_id, cluster_state, hub);
        } else {
            return S2sEventAction::Close;
        }

        state.raw_data = state.raw_data[4 + total_len..].to_vec();
    }
}

/// Inicia as threads de leitura e escrita para um peer conectado.
/// Retorna o Sender para enviar envelopes para a thread de escrita deste peer.
pub fn spawn_peer_threads(
    stream: TcpStream,
    remote_node_id: u8,
    cluster_state: ClusterState,
    hub: WsHub,
    reactor: Arc<crate::reactor::Reactor>,
    pool: Arc<crate::thread_pool::ThreadPool>,
) -> mpsc::Sender<Arc<[u8]>> {
    let (tx, rx) = mpsc::channel::<Arc<[u8]>>();

    let mut write_stream = match stream.try_clone() {
        Ok(s) => s,
        Err(_) => return tx,
    };
    let _ = thread::Builder::new().stack_size(32 * 1024).spawn(move || {
        for data in rx {
            if write_stream.write_all(&data).is_err() {
                break;
            }
        }
    });

    if let Err(_) = stream.set_nonblocking(true) {
        return tx;
    }

    let stream_fd = stream.as_raw_fd();
    let state = Arc::new(Mutex::new(S2sConnectionState {
        stream,
        raw_data: Vec::new(),
    }));

    let reactor_clone = Arc::clone(&reactor);
    let _ = reactor.register(
        stream_fd,
        crate::reactor::EPOLLIN | crate::reactor::EPOLLONESHOT,
        move |_, g| {
            let st_exec = Arc::clone(&state);
            let r_exec = Arc::clone(&reactor_clone);
            let cs_exec = cluster_state.clone();
            let h_exec = hub.clone();

            if let Err(_) = pool.execute(move || {
                let action = process_s2s_event(&st_exec, &cs_exec, &h_exec, remote_node_id);
                match action {
                    S2sEventAction::KeepAlive => {
                        let _ = r_exec.modify(
                            stream_fd,
                            crate::reactor::EPOLLIN | crate::reactor::EPOLLONESHOT,
                        );
                    }
                    S2sEventAction::Close => {
                        let _ = r_exec.unregister_generation(stream_fd, g);
                        cs_exec.unregister_peer(remote_node_id);
                    }
                }
            }) {
                // ThreadPool exhausted
                let _ = reactor_clone.unregister_generation(stream_fd, g);
                cluster_state.unregister_peer(remote_node_id);
            }
        },
    );

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
                    env.target[0],
                    env.target[1],
                    env.target[2],
                    env.target[3],
                    env.target[4],
                    env.target[5],
                    env.target[6],
                    env.target[7],
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
                    env.target[0],
                    env.target[1],
                    env.target[2],
                    env.target[3],
                    env.target[4],
                    env.target[5],
                    env.target[6],
                    env.target[7],
                ]);
                let room = String::from_utf8_lossy(&env.target[8..]).to_string();
                hub.broadcast_to_room_except_local_raw(&room, exclude_id, &env.payload);
                cluster_state.forward_to_all_peers_except(&env, from_node_id);
            }
        }

        S2sMessageType::SendTo => {
            if env.target.len() >= 8 {
                let target_id = u64::from_be_bytes([
                    env.target[0],
                    env.target[1],
                    env.target[2],
                    env.target[3],
                    env.target[4],
                    env.target[5],
                    env.target[6],
                    env.target[7],
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
                    env.payload[1],
                    env.payload[2],
                    env.payload[3],
                    env.payload[4],
                    env.payload[5],
                    env.payload[6],
                    env.payload[7],
                    env.payload[8],
                ]);
                if action == 1 {
                    cluster_state.register_remote_presence(user_id, env.node_origin);
                } else {
                    cluster_state.unregister_remote_presence(user_id);
                }

                cluster_state.forward_to_all_peers_except(&env, from_node_id);
            }
        }
        S2sMessageType::Leave => {
            cluster_state.remove_peer(env.node_origin);
            cluster_state.forward_to_all_peers_except(&env, from_node_id);
        }
    }
}

/// Realiza o handshake inicial entre dois nos do cluster.
/// O no que inicia a conexao envia seu node_id como primeiro byte,
/// seguido pelo HMAC_SHA1 do node_id se um secret estiver configurado.
pub fn send_handshake(stream: &mut TcpStream, my_node_id: u8, secret: Option<&[u8]>) -> bool {
    let mut data = vec![my_node_id];
    if let Some(key) = secret {
        let mac = creeptography::sha1::hmac_sha1(key, &[my_node_id]);
        data.extend_from_slice(&mac);
    }
    stream.write_all(&data).is_ok()
}

/// Le o handshake do peer (1 byte: node_id, opcional 20 bytes HMAC)
pub fn read_handshake(stream: &mut TcpStream, secret: Option<&[u8]>) -> Option<u8> {
    let mut buf = [0u8; 1];
    use std::io::Read;
    if stream.read_exact(&mut buf).is_err() {
        return None;
    }
    let node_id = buf[0];

    if let Some(key) = secret {
        let mut mac_buf = [0u8; 20];
        if stream.read_exact(&mut mac_buf).is_err() {
            return None; // Conexao caiu ou peer malicioso
        }
        let expected_mac = creeptography::sha1::hmac_sha1(key, &[node_id]);
        let mut diff = 0;
        for (x, y) in expected_mac.iter().zip(mac_buf.iter()) {
            diff |= x ^ y;
        }
        if diff != 0 {
            return None; // Spoofing detectado no Handshake
        }
    }

    Some(node_id)
}
