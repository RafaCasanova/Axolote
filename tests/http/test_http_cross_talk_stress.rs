extern crate axolote;

use axolote::http::{HttpMethod, HttpRequest, HttpResponse};
use axolote::Server;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn echo_handler(mut req: HttpRequest) -> HttpResponse {
    let nonce = req.query_params.remove("nonce").unwrap_or_default();
    let id = req.query_params.remove("id").unwrap_or_default();
    
    HttpResponse::ok(format!("REPLY:{} FOR {}", nonce, id))
}

fn main() {
    let port = "9021";
    
    // Inicia o servidor em background
    thread::spawn(move || {
        let mut server = Server::new(port);
        server.add_route(HttpMethod::GET, "/echo", echo_handler);
        server.run();
    });
    
    // Espera o servidor subir
    thread::sleep(Duration::from_millis(500));
    
    println!("Iniciando teste de cross-talk HTTP (vazamento de requisicoes)...");
    
    let num_clients = 100;
    let requests_per_client = 50; // 5.000 conexões HTTP completas no total
    
    let mut handles = vec![];
    let success_count = Arc::new(Mutex::new(0));
    let error_logs = Arc::new(Mutex::new(Vec::new()));
    
    for client_idx in 0..num_clients {
        let port_clone = port.to_string();
        let success_clone = Arc::clone(&success_count);
        let error_clone = Arc::clone(&error_logs);
        
        handles.push(thread::spawn(move || {
            let my_id = format!("CLIENT_{}", client_idx);
            let mut local_successes = 0;
            
            for i in 0..requests_per_client {
                let nonce = format!("MSG_{}", i);
                
                let mut stream = match TcpStream::connect(format!("127.0.0.1:{}", port_clone)) {
                    Ok(s) => s,
                    Err(e) => {
                        error_clone.lock().unwrap().push(format!("Falha ao conectar no cliente {}: {}", my_id, e));
                        break;
                    }
                };
                
                let req = format!(
                    "GET /echo?nonce={}&id={} HTTP/1.1\r\n\
                     Host: 127.0.0.1:{}\r\n\
                     Connection: close\r\n\r\n",
                    nonce, my_id, port_clone
                );
                
                if let Err(e) = stream.write_all(req.as_bytes()) {
                    error_clone.lock().unwrap().push(format!("Falha ao escrever requisição no cliente {}: {}", my_id, e));
                    break;
                }
                
                let mut buf = Vec::new();
                if let Err(e) = stream.read_to_end(&mut buf) {
                    error_clone.lock().unwrap().push(format!("Falha ao ler resposta no cliente {}: {}", my_id, e));
                    break;
                }
                
                let response = String::from_utf8_lossy(&buf);
                
                // Pula os headers para ler o body
                let body = if let Some(idx) = response.find("\r\n\r\n") {
                    &response[idx + 4..]
                } else {
                    ""
                };
                
                let expected = format!("REPLY:{} FOR {}", nonce, my_id);
                if body != expected {
                    error_clone.lock().unwrap().push(format!(
                        "CROSSTALK DETECTADO no client_idx {}: Esperava '{}', Recebeu '{}'\nHTTP COMPLETO:\n{}",
                        client_idx, expected, body, response
                    ));
                    break;
                } else {
                    local_successes += 1;
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
        println!("SUCESSO! Nenhuma resposta HTTP vazou para o cliente errado.");
        let total = *success_count.lock().unwrap();
        println!("Total de requisições-respostas HTTP isoladas com sucesso: {}", total);
        assert_eq!(total, num_clients * requests_per_client);
    }
}
