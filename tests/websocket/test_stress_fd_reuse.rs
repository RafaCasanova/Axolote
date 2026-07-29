extern crate axolote;
use axolote::Server;
use axolote::ws::{WsMode};
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

fn create_http_client(port: u16) -> TcpStream {
    let mut stream = TcpStream::connect(format!("127.0.0.1:{}", port)).expect("Falha ao conectar HTTP");
    let req = format!(
        "GET /api/ping HTTP/1.1\r\n\
         Host: 127.0.0.1:{}\r\n\
         Connection: close\r\n\r\n", 
         port
    );
    stream.write_all(req.as_bytes()).unwrap();
    stream
}

fn main() {
    println!("[RUN] test_stress_fd_reuse");

    let port = 8090;
    
    // Inicia o servidor em background
    thread::spawn(move || {
        let mut server = Server::new(&format!("{}", port));
        
        server.add_route(axolote::http::HttpMethod::GET, "/api/ping", |_req: axolote::http::HttpRequest| {
            axolote::http::HttpResponse::ok("pong".to_string())
        });

        server.add_ws_route("/ws", WsMode::Both, |conn, _hub| {
            // Callback vazio, só aceita a conexão e permite fechar
            conn.on_close(|_, _, _| {});
        });
        
        server.run();
    });

    // Aguarda o servidor subir
    thread::sleep(Duration::from_millis(500));

    let num_threads = 20;
    let iterations_per_thread = 50;
    let mut handles = vec![];
    let success_count = Arc::new(AtomicUsize::new(0));

    println!("[TESTE] Iniciando {} threads disparando conexões WS->HTTP sequenciais agressivamente...", num_threads);

    for _ in 0..num_threads {
        let success_count = Arc::clone(&success_count);
        let h = thread::spawn(move || {
            for _ in 0..iterations_per_thread {
                // Passo 1: Abre conexão WebSocket e faz handshake
                let ws_stream = create_ws_client(port);
                
                // Passo 2: Fecha abruptamente o WebSocket (isso despacha o unregister assíncrono no servidor)
                drop(ws_stream);
                
                // Passo 3: Imediatamente abre uma nova conexão HTTP.
                // O kernel provavelmente reaproveitará o mesmo file descriptor (FD).
                let mut http_stream = create_http_client(port);
                
                // Define timeout de 1 segundo para a resposta HTTP para detectar travamentos
                http_stream.set_read_timeout(Some(Duration::from_millis(1000))).unwrap();
                
                let mut buf = [0u8; 1024];
                let n = match http_stream.read(&mut buf) {
                    Ok(n) => n,
                    Err(e) => {
                        panic!("Travamento ou Erro no HTTP (Race Condition no FD?): {:?}", e);
                    }
                };
                
                let resp = String::from_utf8_lossy(&buf[..n]);
                if !resp.contains("200 OK") || !resp.contains("pong") {
                    panic!("Resposta HTTP incorreta: {}", resp);
                }

                success_count.fetch_add(1, Ordering::SeqCst);
            }
        });
        handles.push(h);
    }

    for h in handles {
        h.join().unwrap();
    }

    let total = success_count.load(Ordering::SeqCst);
    assert_eq!(total, num_threads * iterations_per_thread);
    println!("✅ SUCESSO: {} requisições HTTP servidas com sucesso sem nenhum timeout por race condition de FD!", total);
    println!("[OK]  test_stress_fd_reuse");
}
