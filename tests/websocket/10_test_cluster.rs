extern crate axolote;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use axolote::prelude::*;

fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    conn.join("lobby");
    while let Some(msg) = conn.receive() {
        if let WsMessage::Text(text) = msg {
            hub.broadcast_to_room("lobby", &text);
        }
    }
}

fn create_client(port: &str, name: &str) -> TcpStream {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    let req = format!(
        "GET /chat HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\r\n",
        port
    );
    stream.write_all(req.as_bytes()).unwrap();
    
    let mut buf = [0; 1024];
    stream.read(&mut buf).unwrap(); // Ignora a resposta 101 do server
    stream
}

fn read_ws_frame(stream: &mut TcpStream) -> String {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).unwrap();
    let mut payload_len = (header[1] & 0x7F) as usize;
    if payload_len == 126 {
        let mut ext = [0u8; 2];
        stream.read_exact(&mut ext).unwrap();
        payload_len = u16::from_be_bytes(ext) as usize;
    }
    let mut payload = vec![0u8; payload_len];
    stream.read_exact(&mut payload).unwrap();
    String::from_utf8_lossy(&payload).to_string()
}

fn send_ws_frame(stream: &mut TcpStream, text: &str) {
    let bytes = text.as_bytes();
    let mut frame = Vec::new();
    frame.push(0x81); // FIN + Text
    
    let len = bytes.len();
    frame.push((len as u8) | 0x80); // Mask bit set
    frame.extend_from_slice(&[0, 0, 0, 0]); // Dummy mask
    frame.extend_from_slice(bytes);
    
    stream.write_all(&frame).unwrap();
}

fn main() {
    // Inicia Node 1
    thread::spawn(|| {
        let mut server = Server::new("8081");
        let mut config = ClusterConfig::new(1, "9001", vec!["127.0.0.1:9002".to_string()]);
        // Acelerar deteccao de heartbeat para o teste (2 segundos)
        config.heartbeat_interval_secs = 1;
        config.heartbeat_missed_limit = 2;
        server.enable_cluster(config);

        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });

    // Inicia Node 2
    thread::spawn(|| {
        let mut server = Server::new("8082");
        let mut config = ClusterConfig::new(2, "9002", vec!["127.0.0.1:9001".to_string()]);
        config.heartbeat_interval_secs = 1;
        config.heartbeat_missed_limit = 2;
        server.enable_cluster(config);

        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });

    println!("[TESTE] Aguardando servidores e S2S (2 segundos)...");
    thread::sleep(Duration::from_secs(2));

    let mut client1 = create_client("8081", "Client_1");
    let mut client2 = create_client("8082", "Client_2");

    println!("[TESTE] Client 1 (Node 1) e Client 2 (Node 2) conectados na sala 'lobby'.");
    
    // Client 1 envia para Node 1
    println!("[TESTE] Client 1 enviando mensagem...");
    send_ws_frame(&mut client1, "Ola Node 2! Sou o Client 1.");

    // Client 2 le do Node 2
    let msg2 = read_ws_frame(&mut client2);
    println!("[TESTE] Client 2 recebeu: {}", msg2);

    if msg2 == "Ola Node 2! Sou o Client 1." {
        println!("✅ SUCESSO: Cluster distribuiu a mensagem corretamente entre os nos S2S!");
    } else {
        println!("❌ FALHA: Mensagem nao chegou ou foi corrompida.");
        std::process::exit(1);
    }
}
