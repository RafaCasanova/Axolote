extern crate axolote;
use axolote::ws::frame::{self, Opcode};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn main() {
    let port = "8089";
    println!("Iniciando ALICE (A Remetente)...");

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha");
    
    let handshake = format!("GET /pm HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n", port);
    stream.write_all(handshake.as_bytes()).unwrap();

    let mut buf = [0u8; 1024];
    stream.read(&mut buf).unwrap();

    println!("ALICE: Conectada! (Deveria ser o ID 3).");

    // Aguarda um instante para garantir que Bob e Eve estejam a postos
    thread::sleep(Duration::from_secs(1));

    // Envia a PM para o Bob (ID 1)
    let msg = "TO:1:Oi Bob! Essa mensagem é estritamente confidencial para você.";
    println!("ALICE: Enviando -> '{}'", msg);
    frame::write_frame(&mut stream, Opcode::Text, msg.as_bytes());

    // Fecha a conexão
    frame::write_frame(&mut stream, Opcode::Close, &1000u16.to_be_bytes());
    println!("ALICE: Mensagem enviada e conexão fechada.");
}
