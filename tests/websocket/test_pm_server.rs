extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};

fn handler_pm(mut conn: WsConnection, hub: WsHub) {
    println!("[Servidor] ID {} conectou.", conn.id());

    loop {
        match conn.receive() {
            Some(WsMessage::Text(msg)) => {
                println!("[Servidor] ID {} enviou: {}", conn.id(), msg);
                
                // Formato esperado: TO:ID:MENSAGEM
                if msg.starts_with("TO:") {
                    let parts: Vec<&str> = msg.splitn(3, ':').collect();
                    if parts.len() == 3 {
                        if let Ok(target_id) = parts[1].parse::<u64>() {
                            let texto = parts[2];
                            let formatted = format!("Mensagem Privada de {}: {}", conn.id(), texto);
                            
                            // ENVIO PRIVADO
                            let success = hub.send_to(target_id, &formatted);
                            
                            if success {
                                println!("[Servidor] PM entregue com sucesso para ID {}.", target_id);
                            } else {
                                println!("[Servidor] Falha ao entregar PM para ID {}.", target_id);
                            }
                        }
                    }
                }
            }
            Some(WsMessage::Close(_)) => {
                println!("[Servidor] ID {} saiu.", conn.id());
                break;
            }
            None => {
                println!("[Servidor] ID {} caiu.", conn.id());
                break;
            }
            _ => {}
        }
    }
}

fn main() {
    let port = "8089";
    println!("Iniciando Servidor de Mensagens Privadas na porta {}...", port);
    
    let mut server = Server::new(port);
    server.add_ws_route("/pm", WsMode::Both, handler_pm);
    server.run();
}
