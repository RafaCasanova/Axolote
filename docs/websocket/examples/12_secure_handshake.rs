extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsHub, WsMode, WsRouteConfig};
use axolote::ws::security::WsSecurityGuard;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

fn main() {
    let mut server = Server::new("127.0.0.1:8080");

    // Cria a configuração de segurança do WebSocket
    let security_guard = WsSecurityGuard::new()
        //.with_origins(vec!["http://localhost:8080", "https://meusistema.com"]) // Descomente para testar bloqueio de Origin
        .with_subprotocols(vec!["chat", "superchat"])
        .with_validator(|req| {
            // Verifica se o token JWT ou auth existe nos query parameters
            // Exemplo: ws://127.0.0.1:8080/ws/secure?token=admin123
            let has_token = req.query_params.get("token").map(|v| v.as_str()) == Some("admin123");
            if !has_token {
                println!("Validação customizada: Token inválido ou ausente.");
            }
            has_token
        });

    let config = WsRouteConfig {
        max_message_size: 65536,
        ping_interval_secs: None,
        pong_timeout_secs: None,
        security: Some(Arc::new(security_guard)),
    };

    server.add_ws_route_with_config("/ws/secure", WsMode::Both, config, |mut conn: WsConnection, _hub: WsHub| {
        println!("Cliente conectado na rota segura! ID: {}", conn.id());
        
        // Envia mensagem de boas vindas
        conn.send("Você está em uma rota hiper-segura!");

        loop {
            match conn.receive() {
                Some(msg) => {
                    println!("Recebido do cliente seguro: {:?}", msg);
                    conn.send("Mensagem segura recebida com sucesso!");
                }
                None => {
                    println!("Cliente seguro desconectou.");
                    break;
                }
            }
        }
    });

    println!("Iniciando servidor de WebSocket SEGURO...");
    println!("Para testar falha (sem token):");
    println!("curl -i -N -H \"Connection: Upgrade\" -H \"Upgrade: websocket\" -H \"Sec-WebSocket-Key: SGVsbG8sIHdvcmxkIQ==\" -H \"Sec-WebSocket-Version: 13\" http://127.0.0.1:8080/ws/secure");
    println!("\nPara testar sucesso (com token e subprotocolo):");
    println!("curl -i -N -H \"Connection: Upgrade\" -H \"Upgrade: websocket\" -H \"Sec-WebSocket-Key: SGVsbG8sIHdvcmxkIQ==\" -H \"Sec-WebSocket-Version: 13\" -H \"Sec-WebSocket-Protocol: chat\" http://127.0.0.1:8080/ws/secure?token=admin123");
    
    server.run();
}
