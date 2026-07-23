extern crate axolote;

use axolote::Server;
use axolote::ws::{WsConnection, WsHub, WsMode, WsRouteConfig};
use axolote::ws::security::WsSecurityGuard;
use std::sync::Arc;
use std::thread;
use std::time::Duration;
use std::io::{Read, Write};
use std::net::TcpStream;

fn main() {
    // 1. Inicia o servidor em uma thread de background
    thread::spawn(|| {
        let mut server = Server::new("8081");

        // Rota Aberta (Sem segurança)
        server.add_ws_route("/ws/public", WsMode::Both, |mut conn: WsConnection, _hub: WsHub| {
            conn.send("Bem-vindo à rota pública!");
            while let Some(_) = conn.receive() {}
        });

        // Rota Protegida (Exige Header e Query)
        let guard = WsSecurityGuard::new()
            .with_query_token("api_key", "supersecreto")
            .with_header_token("Authorization", "Bearer token_mestre");

        let config = WsRouteConfig {
            max_message_size: 65536,
            ping_interval_secs: None,
            pong_timeout_secs: None,
            security: Some(Arc::new(guard)),
            id_extractor: None,
        };

        server.add_ws_route_with_config("/ws/vault", WsMode::Both, config, |mut conn: WsConnection, _hub: WsHub| {
            conn.send("Bem-vindo ao cofre! Você passou em todas as verificações.");
            while let Some(_) = conn.receive() {}
        });

        println!("Servidor de Teste Extremo Iniciado na porta 8081...");
        server.run();
    });

    // Aguarda o servidor subir
    thread::sleep(Duration::from_millis(500));

    println!("--------------------------------------------------");
    println!("INICIANDO BATERIA DE TESTES EXTREMOS TCP");
    println!("--------------------------------------------------");

    let mut failed_tests = 0;

    // Helper para atirar requests
    let send_raw_request = |path: &str, extra_headers: &str| -> String {
        let mut stream = TcpStream::connect("127.0.0.1:8081").expect("Falha ao conectar");
        let req = format!(
            "GET {} HTTP/1.1\r\n\
            Host: 127.0.0.1:8081\r\n\
            Connection: Upgrade\r\n\
            Upgrade: websocket\r\n\
            Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\n\
            Sec-WebSocket-Version: 13\r\n\
            {}\r\n",
            path, extra_headers
        );
        stream.write_all(req.as_bytes()).unwrap();
        
        let mut buf = [0; 1024];
        let bytes_read = stream.read(&mut buf).unwrap_or(0);
        String::from_utf8_lossy(&buf[..bytes_read]).to_string()
    };

    // TESTE 1: Rota pública deve aceitar qualquer coisa
    print!("Teste 1: Acesso Público... ");
    let res = send_raw_request("/ws/public", "");
    if res.contains("101 Switching Protocols") {
        println!("✅ OK (101 Permitido)");
    } else {
        println!("❌ FALHOU! Esperava 101, recebeu:\n{}", res);
        failed_tests += 1;
    }

    // TESTE 2: Rota privada SEM NADA
    print!("Teste 2: Cofre Sem Chaves... ");
    let res = send_raw_request("/ws/vault", "");
    if res.contains("401 Unauthorized") {
        println!("✅ OK (Bloqueado com 401)");
    } else {
        println!("❌ FALHOU! Esperava 401, recebeu:\n{}", res);
        failed_tests += 1;
    }

    // TESTE 3: Rota privada COM QUERY ERRADA
    print!("Teste 3: Cofre com Query Errada... ");
    let res = send_raw_request("/ws/vault?api_key=errado", "");
    if res.contains("401 Unauthorized") {
        println!("✅ OK (Bloqueado com 401)");
    } else {
        println!("❌ FALHOU! Esperava 401, recebeu:\n{}", res);
        failed_tests += 1;
    }

    // TESTE 4: Rota privada COM QUERY CERTA, MAS SEM HEADER
    print!("Teste 4: Cofre com Query Certa e Sem Header... ");
    let res = send_raw_request("/ws/vault?api_key=supersecreto", "");
    if res.contains("401 Unauthorized") {
        println!("✅ OK (Bloqueado com 401)");
    } else {
        println!("❌ FALHOU! Esperava 401, recebeu:\n{}", res);
        failed_tests += 1;
    }

    // TESTE 5: Rota privada COM TUDO CERTO
    print!("Teste 5: Cofre com Header e Query Corretos... ");
    let res = send_raw_request("/ws/vault?api_key=supersecreto", "Authorization: Bearer token_mestre\r\n");
    if res.contains("101 Switching Protocols") {
        println!("✅ OK (101 Permitido)");
    } else {
        println!("❌ FALHOU! Esperava 101, recebeu:\n{}", res);
        failed_tests += 1;
    }

    // TESTE 6: Rota privada COM INJEÇÃO DE QUERY FALSificada (tentativa de bypass)
    print!("Teste 6: Cofre com Query Injetada (&api_key=)... ");
    let res = send_raw_request("/ws/vault?fake=1&api_key=hack", "Authorization: Bearer token_mestre\r\n");
    if res.contains("401 Unauthorized") {
        println!("✅ OK (Bloqueado com 401)");
    } else {
        println!("❌ FALHOU! Esperava 401, recebeu:\n{}", res);
        failed_tests += 1;
    }

    println!("--------------------------------------------------");
    if failed_tests == 0 {
        println!("🎯 SUCESSO ABSOLUTO! O servidor é à prova de balas.");
        std::process::exit(0);
    } else {
        println!("🔥 FALHA CRÍTICA! {} testes de segurança falharam.", failed_tests);
        std::process::exit(1);
    }
}
