extern crate axolote;
use axolote::Server;
use axolote::ws::{WsMode, WsMessage};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn create_ws_client(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha ao conectar WS");
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

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(resp.contains("101 Switching Protocols"));
    stream
}

// Codifica um frame WebSocket de texto com máscara
fn encode_ws_frame(text: &str) -> Vec<u8> {
    let payload = text.as_bytes();
    let mut frame = Vec::new();
    
    // FIN = 1, Opcode = 1 (Text)
    frame.push(0x81);
    
    // Mask = 1
    let len = payload.len();
    if len < 126 {
        frame.push(0x80 | (len as u8));
    } else if len < 65536 {
        frame.push(0x80 | 126);
        frame.push(((len >> 8) & 0xFF) as u8);
        frame.push((len & 0xFF) as u8);
    } else {
        frame.push(0x80 | 127);
        let len_u64 = len as u64;
        for i in (0..8).rev() {
            frame.push(((len_u64 >> (i * 8)) & 0xFF) as u8);
        }
    }
    
    let mask_key = [0x12, 0x34, 0x56, 0x78];
    frame.extend_from_slice(&mask_key);
    
    for (i, &b) in payload.iter().enumerate() {
        frame.push(b ^ mask_key[i % 4]);
    }
    
    frame
}

fn main() {
    println!("[RUN] test_ws_fragmentation_stress");
    let port = 8092;

    // Inicia o servidor em background
    thread::spawn(move || {
        let mut server = Server::new(&format!("{}", port));
        
        server.add_ws_route("/ws", WsMode::Both, |conn, _hub| {
            // Echo server
            conn.on_message(|id, hub, msg| {
                if let WsMessage::Text(text) = msg {
                    hub.send_to(id, &text);
                }
            });
        });
        
        server.run();
    });

    thread::sleep(Duration::from_millis(500));

    let num_clients = 10;
    let payload_size = 50_000; // 50KB payload
    let chunk_size = 1000; // Manda de 1000 em 1000 bytes
    
    // Payload gigante repetindo "Axolote"
    let mut big_text = String::with_capacity(payload_size);
    while big_text.len() < payload_size {
        big_text.push_str("Axolote12345");
    }
    big_text.truncate(payload_size);

    let frame = encode_ws_frame(&big_text);
    
    let success_count = Arc::new(AtomicUsize::new(0));
    let mut handles = vec![];

    println!("[TESTE] {} clientes enviando {} bytes fragmentados em blocos de {} bytes...", num_clients, frame.len(), chunk_size);

    for i in 0..num_clients {
        let success_count = Arc::clone(&success_count);
        let frame_clone = frame.clone();
        let expected_text = big_text.clone();
        
        let h = thread::spawn(move || {
            let mut stream = create_ws_client(port);
            stream.set_nodelay(true).unwrap();

            // Envia o frame fragmentado com sleeps para forçar o WouldBlock/EAGAIN no servidor
            let mut offset = 0;
            while offset < frame_clone.len() {
                let end = std::cmp::min(offset + chunk_size, frame_clone.len());
                stream.write_all(&frame_clone[offset..end]).unwrap();
                stream.flush().unwrap();
                thread::sleep(Duration::from_millis(2));
                offset = end;
            }

            // Lê a resposta do Echo
            stream.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
            let mut header = [0u8; 2];
            stream.read_exact(&mut header).expect("Falha ao ler resposta (Timeout?)");
            
            // É um echo text (FIN=1, Opcode=1, Mask=0 do servidor)
            assert_eq!(header[0], 0x81);
            let mut payload_len = (header[1] & 0x7F) as usize;
            
            if payload_len == 126 {
                let mut ext = [0u8; 2];
                stream.read_exact(&mut ext).unwrap();
                payload_len = ((ext[0] as usize) << 8) | (ext[1] as usize);
            } else if payload_len == 127 {
                let mut ext = [0u8; 8];
                stream.read_exact(&mut ext).unwrap();
                let mut len: u64 = 0;
                for b in ext {
                    len = (len << 8) | (b as u64);
                }
                payload_len = len as usize;
            }

            let mut resp_payload = vec![0u8; payload_len];
            stream.read_exact(&mut resp_payload).unwrap();

            let resp_text = String::from_utf8(resp_payload).unwrap();
            assert_eq!(resp_text.len(), expected_text.len());
            
            success_count.fetch_add(1, Ordering::SeqCst);
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    let total = success_count.load(Ordering::SeqCst);
    assert_eq!(total, num_clients);
    println!("✅ SUCESSO: Fragmentação TCP e processamento de grandes payloads funcionando para {} clientes concorrentes sem travar o ThreadPool!", total);
    println!("[OK]  test_ws_fragmentation_stress");
}
