extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};
use axolote::ws::cluster::ClusterConfig;
use std::env;

fn chat_handler(conn: &mut WsConnection, hub: WsHub) {
    conn.join("lobby");
    println!("Novo cliente conectado! ID: {}", conn.id());

    conn.on_message(|id, hub, msg| {
        if let WsMessage::Text(texto) = msg {
            let log_msg = format!("Node original [User {}]: {}", id, texto);
            println!("Broadcast: {}", log_msg);
            hub.broadcast_to_room("lobby", &log_msg);
        }
    });

    conn.on_close(|id, _hub, _code| {
        println!("Cliente desconectado: {}", id);
    });
}

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 4 {
        println!("Uso: ./cluster_simple <NODE_ID> <HTTP_PORT> <S2S_PORT> [SEED_NODE_IP:PORT...]");
        println!("Exemplo: ./cluster_simple 1 8081 9001 127.0.0.1:9002 127.0.0.1:9003");
        return;
    }

    let node_id: u8 = args[1].parse().expect("NODE_ID deve ser um numero entre 1 e 255");
    let http_port = &args[2];
    let s2s_port = &args[3];
    
    let mut seeds = Vec::new();
    for i in 4..args.len() {
        seeds.push(args[i].clone());
    }

    let mut server = Server::new(http_port);
    let config = ClusterConfig::new(node_id, s2s_port, seeds.clone());
    server.enable_cluster(config);

    server.add_ws_route("/chat", WsMode::Both, chat_handler);

    println!("Iniciando Cluster Node {}!", node_id);
    println!("- HTTP ouvindo em: {}", http_port);
    println!("- S2S ouvindo em: {}", s2s_port);
    println!("- Conectando em seeds: {:?}", seeds);
    println!("Conecte um cliente websocket em ws://127.0.0.1:{}/chat", http_port);

    server.run();
}
