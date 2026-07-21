extern crate axolote;
use axolote::prelude::*;

fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    let client_id = conn.id();
    
    // Adiciona o cliente a uma sala especifica
    conn.join("lobby");
    conn.set_metadata("nick", &format!("User_{}", client_id));

    // Broadcast para informar que um novo cliente se juntou a sala
    hub.broadcast_to_room("lobby", &format!("Sistema: {} entrou no lobby.", conn.get_metadata("nick").unwrap()));

    loop {
        match conn.receive() {
            Some(WsMessage::Text(texto)) => {
                let nick = conn.get_metadata("nick").unwrap_or_else(|| "Desconhecido".to_string());
                
                // Distribui a mensagem recebida para todos na sala
                let broadcast_msg = format!("{}: {}", nick, texto);
                hub.broadcast_to_room("lobby", &broadcast_msg);
            }
            Some(WsMessage::Close(_)) => {
                let nick = conn.get_metadata("nick").unwrap_or_else(|| "Desconhecido".to_string());
                hub.broadcast_to_room("lobby", &format!("Sistema: {} saiu do lobby.", nick));
                break;
            }
            None => {
                // Conexao perdida abruptamente (tratamento de queda)
                break;
            }
            _ => {}
        }
    }
}

fn main() {
    let mut server = Server::new("8080");

    // Adiciona a rota WebSocket do chat
    server.add_ws_route("/chat", WsMode::Both, chat_handler);

    server.run();
}
