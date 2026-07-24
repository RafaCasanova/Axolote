extern crate axolote;
use axolote::ws::frame::{self, Opcode};
use std::io::{Read, Write};
use std::net::TcpStream;
use std::thread;
use std::time::Duration;

fn main() {
    let port = "8088";
    println!("Iniciando Cliente Ativo na porta {}...", port);

    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha ao conectar ao servidor.");
    
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

    println!("Conectado à sala!");

    // Vamos aguardar 1 segundo antes de enviar a mensagem para garantir que os passivos já conectaram
    println!("Aguardando 1 segundo...");
    thread::sleep(Duration::from_secs(1));

    let msg = "Ola! Esta eh uma mensagem secreta de teste enviada pelo Cliente Ativo.";
    println!("Enviando: '{}'", msg);

    frame::write_client_frame(&mut stream, Opcode::Text, msg.as_bytes());

    // Vamos enviar o frame de close pra sair bonitinho
    frame::write_client_frame(&mut stream, Opcode::Close, &1000u16.to_be_bytes());
    
    println!("Mensagem enviada e conexão fechada com sucesso!");
}
