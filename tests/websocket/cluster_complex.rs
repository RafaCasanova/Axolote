extern crate axolote;
use axolote::prelude::*;
use std::env;

/// Handler complexo mostrando roteamento de mensagens privadas, salas multiplas
/// e uso de metadados na malha de cluster S2S.
fn complex_chat_handler(mut conn: WsConnection, hub: WsHub) {
    // Sala padrao
    conn.join("global");
    conn.set_metadata("nick", &format!("User_{}", conn.id()));

    let join_msg = format!(">>> {} entrou no chat global <<<", conn.get_metadata("nick").unwrap());
    hub.broadcast_to_room("global", &join_msg);

    while let Some(msg) = conn.receive() {
        if let WsMessage::Text(texto) = msg {
            let nick = conn.get_metadata("nick").unwrap();
            let mut parts = texto.splitn(3, ' ');
            let command = parts.next().unwrap_or("");

            match command {
                "/nick" => {
                    if let Some(new_nick) = parts.next() {
                        let old_nick = nick.clone();
                        conn.set_metadata("nick", new_nick);
                        let alert = format!("*** {} mudou o nome para {} ***", old_nick, new_nick);
                        hub.broadcast_to_room("global", &alert);
                    } else {
                        conn.send("Uso: /nick <novo_nome>");
                    }
                }
                "/join" => {
                    if let Some(room) = parts.next() {
                        conn.join(room);
                        conn.send(&format!("Você entrou na sala '{}'", room));
                    }
                }
                "/leave" => {
                    if let Some(room) = parts.next() {
                        conn.leave(room);
                        conn.send(&format!("Você saiu da sala '{}'", room));
                    }
                }
                "/pm" => {
                    if let Some(target_id_str) = parts.next() {
                        if let Ok(target_id) = target_id_str.parse::<u64>() {
                            let pm_msg = parts.next().unwrap_or("");
                            let formatted_pm = format!("[PM de {}]: {}", nick, pm_msg);
                            
                            // Tenta enviar para o alvo (pode estar em qualquer nó do cluster!)
                            let delivered = hub.send_to(target_id, &formatted_pm);
                            if delivered {
                                conn.send(&format!("PM enviado para {}.", target_id));
                            } else {
                                conn.send(&format!("Usuário {} não encontrado ou rede inoperante.", target_id));
                            }
                        }
                    } else {
                        conn.send("Uso: /pm <id> <mensagem>");
                    }
                }
                "/shout" => {
                    // Envia para todos na sala global, EXCETO eu mesmo
                    let shout = parts.collect::<Vec<&str>>().join(" ");
                    let formatted_shout = format!("📢 {} Grita: {}", nick, shout);
                    hub.broadcast_to_room_except("global", conn.id(), &formatted_shout);
                    conn.send("Grito ecoado pela malha global!");
                }
                _ => {
                    // Mensagem padrao enviada ao global
                    let formatted = format!("{}: {}", nick, texto);
                    hub.broadcast_to_room("global", &formatted);
                }
            }
        }
    }
    
    let leave_msg = format!("<<< {} desconectou <<<", conn.get_metadata("nick").unwrap());
    hub.broadcast_to_room("global", &leave_msg);
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Uso: ./cluster_complex <NODE_ID> <HTTP_PORT> <S2S_PORT> [SEED_NODE_IP:PORT...]");
        return;
    }

    let node_id: u8 = args[1].parse().unwrap();
    let http_port = &args[2];
    let s2s_port = &args[3];
    
    let mut seeds = Vec::new();
    for i in 4..args.len() {
        seeds.push(args[i].clone());
    }

    let mut server = Server::new(http_port);
    let mut config = ClusterConfig::new(node_id, s2s_port, seeds.clone());
    
    // Configuracoes agressivas pro ambiente complexo
    config.heartbeat_interval_secs = 3;
    config.heartbeat_missed_limit = 3;
    server.enable_cluster(config);

    server.add_ws_route("/complex", WsMode::Both, complex_chat_handler);

    println!("Cluster Avançado Iniciado!");
    println!("Dica: Conecte múltiplos clientes, tente /pm <id_de_outro_cluster> <msg>");
    println!("Ou teste /shout e /join!");
    
    server.run();
}
