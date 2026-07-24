extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};

fn chat_handler(conn: &mut WsConnection, hub: WsHub) {
    let client_id = conn.id();
    
    // Adiciona o cliente a uma sala especifica
    hub.join_room(client_id, "lobby");
    hub.set_client_metadata(client_id, "nick", &format!("User_{}", client_id));

    // Broadcast para informar que um novo cliente se juntou a sala
    hub.broadcast_to_room("lobby", &format!("Sistema: {} entrou no lobby.", hub.get_client_metadata(client_id, "nick").unwrap()));

    conn.on_message(|_id, hub_ref, msg| {
        if let WsMessage::Text(texto) = msg {
            let nick = hub_ref.get_client_metadata(_id, "nick").unwrap_or_else(|| "Desconhecido".to_string());
            
            // Distribui a mensagem recebida para todos na sala
            let broadcast_msg = format!("{}: {}", nick, texto);
            hub_ref.broadcast_to_room("lobby", &broadcast_msg);
        }
    });

    conn.on_close(|_id, hub_ref, _code| {
        let nick = hub_ref.get_client_metadata(_id, "nick").unwrap_or_else(|| "Desconhecido".to_string());
        hub_ref.broadcast_to_room("lobby", &format!("Sistema: {} saiu do lobby.", nick));
    });
}

fn main() {
    let mut server = Server::new("8080");

    // Adiciona a rota WebSocket do chat
    server.add_ws_route("/chat", WsMode::Both, chat_handler);

    server.run();
}
