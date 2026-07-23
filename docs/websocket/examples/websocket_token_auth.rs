extern crate axolote;

use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub, WsRouteConfig};
use axolote::ws::security::WsSecurityGuard;
use std::sync::Arc;

fn secure_chat_handler(mut conn: WsConnection, _hub: WsHub) {
    conn.send("Acesso Autorizado! Você passou por todas as barreiras de segurança do Handshake.");

    while let Some(msg) = conn.receive() {
        match msg {
            WsMessage::Text(t) => {
                println!("Cliente [{}] enviou: {}", conn.id(), t);
                conn.send(&format!("Recebido: {}", t));
            }
            _ => {}
        }
    }
}

fn main() {
    let mut server = Server::new("8080");

    let security_guard = WsSecurityGuard::new()
        .with_origins(vec!["https://meu-painel.com", "http://localhost:8080"])
        .with_query_token("api_key", "SENHA_123")
        .with_header_token("X-Admin-Token", "TOKEN_VALIDO")
        .with_validator(|req| {
            // Rejeita a conexão caso o path contenha a palavra "hacker"
            !req.path.contains("hacker")
        });

    let mut config = WsRouteConfig::default();
    config.security = Some(Arc::new(security_guard));

    server.add_ws_route_with_config("/secure_chat", WsMode::Both, config, secure_chat_handler);

    println!("Iniciando servidor WebSocket seguro na porta 8080...");
    println!("Para conectar com sucesso, você precisa:");
    println!("1. Usar Origin: http://localhost:8080");
    println!("2. Passar ?api_key=SENHA_123 na URL");
    println!("3. Passar o header X-Admin-Token: TOKEN_VALIDO");
    
    server.run();
}
