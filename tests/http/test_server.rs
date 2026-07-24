extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};

fn handler_sala(conn: &mut WsConnection, hub: WsHub) {
    conn.join("teste_sala");

    conn.on_message(|id, hub, msg| {
        if let WsMessage::Text(text) = msg {
            println!("[Servidor] ID {} enviou para a sala: {}", id, text);
            hub.broadcast_to_room("teste_sala", &text);
        }
    });

    conn.on_close(|id, _hub, _code| {
        println!("[Servidor] ID {} saiu ou desconectou de forma inesperada.", id);
    });
}

fn main() {
    let port = "8088";
    println!("Iniciando Servidor de Sala de Bate-Papo na porta {}...", port);

    let mut server = Server::new(port);
    server.add_ws_route("/sala", WsMode::Both, handler_sala);
    server.run();
}
