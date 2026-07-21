extern crate axolote;
use axolote::prelude::*;
use axolote::ws::frame::{self, Opcode};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;
use std::sync::{Arc, Mutex};

/// Handler do servidor
fn handler_sala(mut conn: WsConnection, hub: WsHub) {
    conn.join("teste_sala");

    loop {
        match conn.receive() {
            Some(WsMessage::Text(msg)) => {
                // Ao receber, faz o broadcast na sala
                hub.broadcast_to_room("teste_sala", &msg);
            }
            Some(WsMessage::Close(_)) | None => break,
            _ => {}
        }
    }
}

/// Cria um cliente TCP simples que faz o handshake WS
fn create_test_client(port: &str) -> TcpStream {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha ao conectar");
    
    // Handshake HTTP
    let handshake = format!(
        "GET /sala HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
         Sec-WebSocket-Version: 13\r\n\r\n", 
         port
    );
    stream.write_all(handshake.as_bytes()).unwrap();

    // Aguarda a resposta (101 Switching Protocols)
    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    assert!(resp.contains("101 Switching Protocols"), "Handshake falhou");

    stream
}

fn main() {
    let port = "8088";

    // 1. Inicia o servidor em uma thread de background
    thread::spawn(move || {
        let mut server = Server::new(port);
        // O logger padrão pode sujar a saída do teste, mas vamos deixar
        server.add_ws_route("/sala", WsMode::Both, handler_sala);
        server.run();
    });

    // Dá um tempo para o servidor subir
    thread::sleep(Duration::from_millis(500));
    println!("\n[TESTE] Servidor iniciado.");

    // 2. Conecta os dois clientes passivos
    let mut cliente1 = create_test_client(port);
    let mut cliente2 = create_test_client(port);
    println!("[TESTE] Cliente 1 e Cliente 2 conectados.");

    let msgs_c1 = Arc::new(Mutex::new(Vec::new()));
    let msgs_c2 = Arc::new(Mutex::new(Vec::new()));

    // Thread para o Cliente 1 escutar (usa o read_frame do próprio framework)
    let msgs_c1_clone = msgs_c1.clone();
    let mut c1_stream = cliente1.try_clone().unwrap();
    thread::spawn(move || {
        while let Some(f) = frame::read_frame(&mut c1_stream, 65536) {
            if f.opcode == Opcode::Text {
                let text = String::from_utf8_lossy(&f.payload).to_string();
                msgs_c1_clone.lock().unwrap().push(text);
            }
        }
    });

    // Thread para o Cliente 2 escutar
    let msgs_c2_clone = msgs_c2.clone();
    let mut c2_stream = cliente2.try_clone().unwrap();
    thread::spawn(move || {
        while let Some(f) = frame::read_frame(&mut c2_stream, 65536) {
            if f.opcode == Opcode::Text {
                let text = String::from_utf8_lossy(&f.payload).to_string();
                msgs_c2_clone.lock().unwrap().push(text);
            }
        }
    });

    // 3. Conecta o cliente ativo
    let mut cliente3 = create_test_client(port);
    println!("[TESTE] Cliente 3 conectado.");

    // Dá um tempinho pros listeners subirem
    thread::sleep(Duration::from_millis(200));

    // 4. Cliente 3 envia a mensagem para a sala
    let msg_secreta = "Mensagem do Cliente 3 - BROADCAST FUNCIONOU!";
    println!("[TESTE] Cliente 3 enviando: '{}'", msg_secreta);
    
    // O cliente envia um frame de texto. No RFC o cliente devia fazer mask, mas
    // o nosso server aceita unmasked (já que no read_frame ele só faz unmask se o bit estiver setado).
    // Usamos o write_frame do framework que envia unmasked.
    frame::write_frame(&mut cliente3, Opcode::Text, msg_secreta.as_bytes());

    // 5. Aguarda a propagação
    thread::sleep(Duration::from_millis(500));

    // 6. Verifica os resultados
    let lock1 = msgs_c1.lock().unwrap();
    let lock2 = msgs_c2.lock().unwrap();

    let c1_recebeu = lock1.iter().any(|m| m == msg_secreta);
    let c2_recebeu = lock2.iter().any(|m| m == msg_secreta);

    if c1_recebeu && c2_recebeu {
        println!("\n✅ SUCESSO: Cliente 1 e Cliente 2 receberam a mensagem!");
        std::process::exit(0);
    } else {
        println!("\n❌ FALHA: A mensagem não chegou nos dois clientes.");
        println!("Cliente 1 recebeu: {:?}", *lock1);
        println!("Cliente 2 recebeu: {:?}", *lock2);
        std::process::exit(1);
    }
}
