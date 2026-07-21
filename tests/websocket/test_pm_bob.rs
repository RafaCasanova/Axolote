extern crate axolote;
use axolote::ws::frame::{self, Opcode};
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    let port = "8089";
    println!("Iniciando BOB (Alvo da PM)...");

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha");
    
    let handshake = format!("GET /pm HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n", port);
    stream.write_all(handshake.as_bytes()).unwrap();

    let mut buf = [0u8; 1024];
    stream.read(&mut buf).unwrap();

    println!("BOB: Conectado! (Deveria ser o ID 1). Aguardando mensagem privada...");

    while let Some(f) = frame::read_frame(&mut stream, 65536) {
        if f.opcode == Opcode::Text {
            let text = String::from_utf8_lossy(&f.payload).to_string();
            println!("💬 BOB RECEBEU: {}", text);
        } else if f.opcode == Opcode::Close {
            println!("BOB: Servidor fechou.");
            break;
        }
    }
}
