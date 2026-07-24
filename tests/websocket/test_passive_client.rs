extern crate axolote;
use axolote::ws::frame::{self, Opcode};
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    let port = "8088";
    println!("Iniciando Cliente Passivo na porta {}...", port);

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha ao conectar ao servidor. O servidor está rodando?");
    
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

    let mut buf = [0u8; 1024];
    let n = stream.read(&mut buf).unwrap();
    let resp = String::from_utf8_lossy(&buf[..n]);
    if !resp.contains("101 Switching Protocols") {
        println!("Erro no Handshake: {}", resp);
        return;
    }

    println!("Conectado! Aguardando mensagens...");

    // Loop infinito lendo mensagens
    while let Some(f) = frame::read_frame(&mut stream, 65536, false) {
        if f.opcode == Opcode::Text {
            let text = String::from_utf8_lossy(&f.payload).to_string();
            println!("💬 Recebido via Broadcast: {}", text);
        } else if f.opcode == Opcode::Close {
            println!("Servidor fechou a conexão.");
            break;
        }
    }
}
