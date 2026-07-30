extern crate axolote;
use axolote::http::HttpMethod;
use axolote::ws::{WsConnection, WsHub, WsMessage, WsMode};
use axolote::Server;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

// Funções de cliente de teste extraídas de testes anteriores
fn create_test_client(port: &str) -> TcpStream {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).unwrap();
    let handshake = format!(
        "GET /ws HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n",
        port
    );
    stream.write_all(handshake.as_bytes()).unwrap();

    let mut response = String::new();
    let mut byte = [0u8; 1];
    while let Ok(1) = stream.read(&mut byte) {
        response.push(byte[0] as char);
        if response.ends_with("\r\n\r\n") {
            break;
        }
    }
    assert!(response.contains("101 Switching Protocols"));
    stream
}

fn write_ws_frame(stream: &mut TcpStream, payload: &str) {
    let mut frame = Vec::new();
    frame.push(0x81); // FIN + Text
    
    let len = payload.len();
    if len <= 125 {
        frame.push((len as u8) | 0x80); // Mask bit set
    } else if len <= 65535 {
        frame.push(126 | 0x80);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127 | 0x80);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }
    
    let mask = [0x12, 0x34, 0x56, 0x78];
    frame.extend_from_slice(&mask);
    
    let mut masked_payload = payload.as_bytes().to_vec();
    for i in 0..masked_payload.len() {
        masked_payload[i] ^= mask[i % 4];
    }
    frame.extend_from_slice(&masked_payload);
    
    stream.write_all(&frame).unwrap();
}

fn read_ws_frame(stream: &mut TcpStream) -> Option<String> {
    let mut header = [0u8; 2];
    if stream.read_exact(&mut header).is_err() {
        return None;
    }
    
    let opcode = header[0] & 0x0F;
    if opcode == 0x8 {
        return None; // Close frame
    }
    
    let mut len = (header[1] & 0x7F) as usize;
    if len == 126 {
        let mut ext_len = [0u8; 2];
        stream.read_exact(&mut ext_len).unwrap();
        len = u16::from_be_bytes(ext_len) as usize;
    } else if len == 127 {
        let mut ext_len = [0u8; 8];
        stream.read_exact(&mut ext_len).unwrap();
        len = u64::from_be_bytes(ext_len) as usize;
    }
    
    let mut payload = vec![0u8; len];
    if stream.read_exact(&mut payload).is_err() {
        return None;
    }
    
    if opcode == 0x1 {
        Some(String::from_utf8_lossy(&payload).to_string())
    } else {
        None
    }
}

fn chat_handler(conn: &mut WsConnection, hub: WsHub) {
    let my_id = conn.id();
    
    // Envia o ID para o cliente logo na conexão
    hub.send_to(my_id, &format!("HELLO:{}", my_id));

    conn.on_message(move |id, hub, msg| {
        if let WsMessage::Text(t) = msg {
            if t.starts_with("ECHO:") {
                let nonce = &t[5..];
                // Simula processamento
                // Responde estritamente para o ID solicitante
                hub.send_to(id, &format!("REPLY:{} FOR {}", nonce, id));
            } else if t.starts_with("BROADCAST:") {
                // Testa vazamento em broadcast - manda pra sala
                hub.broadcast_to_room("lobby", &format!("BROADCAST_REPLY:{}", id));
            }
        }
    });
}

fn main() {
    let port = "9019";
    
    // Inicia o servidor em background
    thread::spawn(move || {
        let mut server = Server::new(port);
        server.add_ws_route("/ws", WsMode::Both, chat_handler);
        server.run();
    });
    
    // Espera o servidor subir
    thread::sleep(Duration::from_millis(500));
    
    println!("Iniciando teste de cross-talk (vazamento de IDs)...");
    
    let num_clients = 150;
    let messages_per_client = 100; // 15.000 mensagens totais trafegando simultaneamente
    
    let mut handles = vec![];
    let success_count = Arc::new(Mutex::new(0));
    let error_logs = Arc::new(Mutex::new(Vec::new()));
    
    for client_idx in 0..num_clients {
        let port_clone = port.to_string();
        let success_clone = Arc::clone(&success_count);
        let error_clone = Arc::clone(&error_logs);
        
        handles.push(thread::spawn(move || {
            let mut stream = create_test_client(&port_clone);
            
            // Ler o HELLO e descobrir qual é o nosso ID no servidor
            let hello_msg = read_ws_frame(&mut stream).expect("Falha ao ler HELLO");
            if !hello_msg.starts_with("HELLO:") {
                error_clone.lock().unwrap().push(format!("Client {} received invalid hello: {}", client_idx, hello_msg));
                return;
            }
            let my_id = hello_msg[6..].to_string();
            
            let mut local_successes = 0;
            
            for i in 0..messages_per_client {
                let nonce = format!("MSG_{}_{}", client_idx, i);
                let payload = format!("ECHO:{}", nonce);
                
                write_ws_frame(&mut stream, &payload);
                
                if let Some(resp) = read_ws_frame(&mut stream) {
                    let expected = format!("REPLY:{} FOR {}", nonce, my_id);
                    if resp != expected {
                        error_clone.lock().unwrap().push(format!(
                            "CROSSTALK DETECTADO no client_idx {}: Esperava '{}', Recebeu '{}'",
                            client_idx, expected, resp
                        ));
                        break;
                    } else {
                        local_successes += 1;
                    }
                } else {
                    error_clone.lock().unwrap().push(format!("Client {} (ID {}) conexão caiu prematuramente na MSG {}", client_idx, my_id, i));
                    break;
                }
            }
            
            *success_clone.lock().unwrap() += local_successes;
        }));
    }
    
    for h in handles {
        h.join().unwrap();
    }
    
    let errors = error_logs.lock().unwrap();
    if errors.len() > 0 {
        println!("FALHA! Vazamentos/erros detectados:");
        for e in errors.iter() {
            println!(" - {}", e);
        }
        std::process::exit(1);
    } else {
        println!("SUCESSO! Nenhuma mensagem cruzou para o ID errado.");
        let total = *success_count.lock().unwrap();
        println!("Total de requisições-respostas validadas simultaneamente: {}", total);
        assert_eq!(total, num_clients * messages_per_client);
    }
}
