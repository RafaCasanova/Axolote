extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};
use axolote::ws::cluster::ClusterConfig;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    conn.join("mesh_room");
    while let Some(msg) = conn.receive() {
        if let WsMessage::Text(text) = msg {
            hub.broadcast_to_room("mesh_room", &text);
        }
    }
}

fn create_client(port: &str) -> TcpStream {
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
    stream.read(&mut buf).unwrap(); 
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
    // Topologia em Ciclo (Mesh Relay):
    // Node 1 conecta no Node 2
    // Node 2 conecta no Node 3
    // Node 3 conecta no Node 1
    
    // Inicia Node 1
    thread::spawn(|| {
        let mut server = Server::new("8081");
        let mut config = ClusterConfig::new(1, "9001", vec!["127.0.0.1:9002".to_string()]);
        config.heartbeat_interval_secs = 1;
        server.enable_cluster(config);
        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });

    // Inicia Node 2
    thread::spawn(|| {
        let mut server = Server::new("8082");
        let mut config = ClusterConfig::new(2, "9002", vec!["127.0.0.1:9003".to_string()]);
        config.heartbeat_interval_secs = 1;
        server.enable_cluster(config);
        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });

    // Inicia Node 3
    thread::spawn(|| {
        let mut server = Server::new("8083");
        let mut config = ClusterConfig::new(3, "9003", vec!["127.0.0.1:9001".to_string()]);
        config.heartbeat_interval_secs = 1;
        server.enable_cluster(config);
        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });

    println!("[TESTE] Aguardando formação do Cluster Gossip Mesh (3 segundos)...");
    thread::sleep(Duration::from_secs(3));

    let mut client1 = create_client("8081");
    let mut client3 = create_client("8083");

    println!("[TESTE] Client 1 (Node 1) e Client 3 (Node 3) conectados.");
    
    // Client 1 envia mensagem para Node 1
    // Node 1 propaga para Node 2
    // Node 2 propaga para Node 3 (Relay)
    // Node 3 entrega para Client 3 e propaga de volta para Node 1 (onde e barrado pelo anti-loop)
    
    let msg = "Gossip Protocol Funciona: De Node 1 para Node 3 via Node 2!";
    println!("[TESTE] Client 1 enviando mensagem na rede...");
    send_ws_frame(&mut client1, msg);

    // Tentamos ler no Client 3 (se houver quebra na topologia estrela, a mensagem jamais chegaria, mas no Gossip ela chega)
    // Vamos setar um timeout manual via sleep e non-blocking read? Nao precisa, o receive no teste bloqueia,
    // se falhar o CI trava (ou timeout externo).
    
    let recv = read_ws_frame(&mut client3);
    println!("[TESTE] Client 3 recebeu: {}", recv);

    if recv == msg {
        println!("✅ SUCESSO: Mensagem retransmitida na malha e entregue independentemente da topologia!");
        
        // Verifica que nao ha duplicata enviando mais mensagens rapidas
        println!("[TESTE] Enviando 100 mensagens rapidas para garantir anti-duplicidade e ordem...");
        for i in 0..100 {
            send_ws_frame(&mut client1, &format!("MSG {}", i));
        }
        
        let mut success = true;
        for i in 0..100 {
            let m = read_ws_frame(&mut client3);
            if m != format!("MSG {}", i) {
                success = false;
                break;
            }
        }
        
        if success {
            println!("✅ SUCESSO Absoluto: Nenhuma mensagem foi perdida e nenhuma foi duplicada!");
        } else {
            println!("❌ FALHA: Perda ou corrupcao de pacotes na malha.");
            std::process::exit(1);
        }
        
    } else {
        println!("❌ FALHA: Mensagem nao chegou ou foi corrompida.");
        std::process::exit(1);
    }
}
