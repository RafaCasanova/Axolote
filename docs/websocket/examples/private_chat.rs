extern crate axolote;

use axolote::Server;
use axolote::ws::{WsConnection, WsHub, WsMode, WsRouteConfig};
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

    server.add_ws_route_with_config("/chat", WsMode::Both, ws_config, |conn: &mut WsConnection, _hub: WsHub| {
        // A conexão é estabelecida já portando o ID extraído da query.
        println!("[WS] Conexão estabelecida. User ID vinculado: {}", conn.id());

        conn.on_message_json(|id, hub_ref, msg_res: Result<PrivateMessage, String>| {
            match msg_res {
                Ok(msg) => {
                    let delivered = hub_ref.send_json_to(msg.to, &msg);
                    
                    let resp = ServerResponse {
                        status: if delivered { "OK".to_string() } else { "ERROR".to_string() },
                        message: if delivered { "Mensagem entregue".to_string() } else { "Usuário offline".to_string() },
                    };
                    hub_ref.send_json_to(id, &resp);
                },
                Err(e) => {
                    println!("[PM] JSON Invalido recebido do User {}: {}", id, e);
                }
            }
        });
        
        conn.on_close(|id, _, _| {
            println!("[WS] Conexão encerrada para o User ID: {}.", id);
        });
    });

    println!("Servidor inicializado na porta 8080.");
    println!("Para testar, conecte os clientes passando o parâmetro 'user_id' na query string.");
    println!("Exemplo: ws://localhost:8080/chat?user_id=10");
    server.run();
}
