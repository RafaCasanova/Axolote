extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsHub, WsMode, WsRouteConfig};
use axolote::ws::security::WsSecurityGuard;
use std::sync::Arc;

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
        id_extractor: None,
    };

    server.add_ws_route_with_config("/ws/secure", WsMode::Both, config, |conn: &mut WsConnection, _hub: WsHub| {
        println!("Cliente conectado na rota segura! ID: {}", conn.id());
        
        // Envia mensagem de boas vindas
        conn.send("Você está em uma rota hiper-segura!");

        conn.on_message(|_id, _hub_ref, msg| {
            println!("Recebido do cliente seguro: {:?}", msg);
            // Isso não pode responder diretamente sem a referência ao hub, 
            // no modo antigo o conn.send() mandava, agora usamos o WsHub no event loop!
            // Para consertar o exemplo, vamos logar apenas, e usar o hub_ref.
            _hub_ref.send_to(_id, "Mensagem segura recebida com sucesso!");
        });

        conn.on_close(|_id, _hub_ref, _code| {
            println!("Cliente seguro desconectou.");
        });
    });

    println!("Iniciando servidor de WebSocket SEGURO...");
    println!("Para testar falha (sem token):");
    println!("curl -i -N -H \"Connection: Upgrade\" -H \"Upgrade: websocket\" -H \"Sec-WebSocket-Key: SGVsbG8sIHdvcmxkIQ==\" -H \"Sec-WebSocket-Version: 13\" http://127.0.0.1:8080/ws/secure");
    println!("\nPara testar sucesso (com token e subprotocolo):");
    println!("curl -i -N -H \"Connection: Upgrade\" -H \"Upgrade: websocket\" -H \"Sec-WebSocket-Key: SGVsbG8sIHdvcmxkIQ==\" -H \"Sec-WebSocket-Version: 13\" -H \"Sec-WebSocket-Protocol: chat\" http://127.0.0.1:8080/ws/secure?token=admin123");
    
    server.run();
}
