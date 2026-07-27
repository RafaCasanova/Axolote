extern crate axolote;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use axolote::Server;
use std::sync::Mutex;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};
use axolote::ws::cluster::ClusterConfig;
static TEST_HUB: Mutex<Option<WsHub>> = Mutex::new(None);

fn chat_handler(conn: &mut WsConnection, hub: WsHub) {
    let mut lock = TEST_HUB.lock().unwrap();
    if lock.is_none() {
        *lock = Some(hub.clone());
    }
    
    conn.on_message(move |id, hub, msg| {
        if let WsMessage::Text(text) = msg {
            if text.starts_with("join:") {
                let room = text.split(':').nth(1).unwrap();
                hub.join_room(id, room);
                hub.send_to(id, "joined");
            } else if text == "cleanup" {
                hub.cleanup_empty_rooms();
                hub.send_to(id, "cleaned");
            }
        }
    });
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

fn send_ws_frame(stream: &mut TcpStream, text: &str) {
    let bytes = text.as_bytes();
    let mut frame = Vec::new();
    frame.push(0x81);
    
    let len = bytes.len();
    frame.push((len as u8) | 0x80);
    frame.extend_from_slice(&[0, 0, 0, 0]);
    frame.extend_from_slice(bytes);
    
    stream.write_all(&frame).unwrap();
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

fn main() {
    println!("[RUN] test_room_cleanup");
    
    // Node 1
    thread::spawn(move || {
        let mut server = Server::new("8086");
        let config = ClusterConfig::new(1, "9006", vec!["127.0.0.1:9007".to_string()]);
        server.enable_cluster(config);
        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });
    
    // Node 2
    thread::spawn(move || {
        let mut server = Server::new("8087");
        let config = ClusterConfig::new(2, "9007", vec!["127.0.0.1:9006".to_string()]);
        server.enable_cluster(config);
        server.add_ws_route("/chat", WsMode::Both, chat_handler);
        server.run();
    });
    
    thread::sleep(Duration::from_secs(2)); // wait for startup and S2S connect
    
    // Client connects to Node 1 and joins 'test_room'
    let mut c1 = create_client("8086");
    
    // Espera ate o TEST_HUB ser populado pelo chat_handler (que roda em outra thread)
    let hub1 = loop {
        if let Some(hub) = TEST_HUB.lock().unwrap().as_ref() {
            break hub.clone();
        }
        thread::sleep(Duration::from_millis(10));
    };

    send_ws_frame(&mut c1, "join:test_room");
    assert_eq!(read_ws_frame(&mut c1), "joined");
    
    thread::sleep(Duration::from_millis(500));
    
    // Verify room is registered in cluster
    let count_before = hub1.room_count("test_room");
    assert_eq!(count_before, 1, "Should have 1 client in test_room locally");
    
    assert!(hub1.has_local_room("test_room"), "Room should be in cluster local_rooms");
    
    // Client disconnects
    drop(c1);
    
    thread::sleep(Duration::from_millis(500)); // wait for disconnect process
    
    // When a client drops, unregister is called. unregister already cleans up the room from cluster!
    assert!(!hub1.has_local_room("test_room"), "Room should be auto-cleaned from cluster local_rooms on disconnect");
    
    // Test manual cleanup just to be sure it doesn't crash
    hub1.cleanup_empty_rooms();
    
    println!("✅ SUCESSO: Limpeza de salas vazias operando corretamente!");
}
