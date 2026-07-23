extern crate axolote;

use axolote::Server;
use axolote::ws::{WsMode, WsRouteConfig};
use axolote::axolote_json;

#[axolote_json]
struct PrivateMessage {
    to: u64,
    text: String,
}

#[axolote_json]
struct ServerResponse {
    status: String,
    message: String,
}

fn main() {
    let mut server = Server::new("8080");

    // Configura a rota WebSocket para extrair o ID da conexão a partir dos parâmetros
    // da Query String durante o Handshake HTTP. (ex: ws://localhost:8080/chat?user_id=10)
    let ws_config = WsRouteConfig::new().id_from_query("user_id");

    server.add_ws_route_with_config("/chat", WsMode::Both, ws_config, |mut conn, hub| {
        // A conexão é estabelecida já portando o ID extraído da query.
        println!("[WS] Conexão estabelecida. User ID vinculado: {}", conn.id());

        let id = conn.id();
        loop {
            if let Some(msg_res) = conn.receive_json::<PrivateMessage>() {
                match msg_res {
                    Ok(msg) => {
                        let delivered = hub.send_json_to(msg.to, &msg);
                        
                        let resp = ServerResponse {
                            status: if delivered { "OK".to_string() } else { "ERROR".to_string() },
                            message: if delivered { "Mensagem entregue".to_string() } else { "Usuário offline".to_string() },
                        };
                        conn.send_json(&resp);
                    },
                    Err(e) => {
                        println!("[PM] JSON Invalido recebido do User {}: {}", id, e);
                    }
                }
            } else {
                // Conexão fechada
                break;
            }
        }
        
        println!("[WS] Conexão encerrada para o User ID: {}.", conn.id());
    });

    println!("Servidor inicializado na porta 8080.");
    println!("Para testar, conecte os clientes passando o parâmetro 'user_id' na query string.");
    println!("Exemplo: ws://localhost:8080/chat?user_id=10");
    server.run();
}
