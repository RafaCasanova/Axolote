extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};

fn handler_sala(mut conn: WsConnection, hub: WsHub) {
    conn.join("teste_sala");

    loop {
        match conn.receive() {
            Some(WsMessage::Text(msg)) => {
                println!("[Servidor] ID {} enviou para a sala: {}", conn.id(), msg);
                hub.broadcast_to_room("teste_sala", &msg);
            }
            Some(WsMessage::Close(_)) => {
                println!("[Servidor] ID {} saiu.", conn.id());
                break;
            }
            None => {
                println!("[Servidor] ID {} desconectado de forma inesperada.", conn.id());
                break;
            }
            _ => {}
        }
    }
}

fn main() {
    let port = "8088";
    println!("Iniciando Servidor de Sala de Bate-Papo na porta {}...", port);

    let mut server = Server::new(port);
    server.add_ws_route("/sala", WsMode::Both, handler_sala);
    server.run();
}
