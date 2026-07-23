extern crate axolote;
use axolote::Server;
use axolote::ws::{WsRouteConfig, WsMessage, WsMode};

fn main() {
    let mut server = Server::new("8080");

    // Configura a rota WebSocket para extrair o ID da conexão a partir dos parâmetros
    // da Query String durante o Handshake HTTP. (ex: ws://localhost:8080/chat?user_id=10)
    let ws_config = WsRouteConfig::new()
        .with_id_extractor(|req| {
            req.query_params
                .get("user_id")
                .and_then(|id_str| id_str.parse::<u64>().ok())
        });

    server.add_ws_route_with_config("/chat", WsMode::Both, ws_config, |mut conn, hub| {
        // A conexão é estabelecida já portando o ID extraído.
        // Caso o ID já esteja em uso no servidor/cluster, o Handshake é 
        // automaticamente recusado com status HTTP 409 Conflict.
        println!("[WS] Conexão estabelecida. User ID vinculado: {}", conn.id());

        while let Some(msg) = conn.receive() {
            if let WsMessage::Text(texto) = msg {
                // Protocolo simples de roteamento: "target_id:mensagem"
                // Exemplo de payload esperado do cliente: "20:Hello World"
                if let Some((target_id_str, payload)) = texto.split_once(':') {
                    if let Ok(target_id) = target_id_str.parse::<u64>() {
                        
                        // Roteia a mensagem diretamente para o cliente de destino através do Hub.
                        let delivered = hub.send_to(target_id, payload);
                        
                        if delivered {
                            println!("[WS] Mensagem roteada do ID {} para o ID {}.", conn.id(), target_id);
                        } else {
                            println!("[WS] Falha de roteamento: O ID de destino {} não está acessível.", target_id);
                        }
                    }
                }
            }
        }
        
        println!("[WS] Conexão encerrada para o User ID: {}.", conn.id());
    });

    println!("Servidor inicializado na porta 8080.");
    println!("Para testar, conecte os clientes passando o parâmetro 'user_id' na query string.");
    println!("Exemplo: ws://localhost:8080/chat?user_id=10");
    server.run();
}
