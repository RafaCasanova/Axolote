extern crate axolote;
use axolote::Server;
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub};

fn handler_pm(conn: &mut WsConnection, hub: WsHub) {
    println!("[Servidor] ID {} conectou.", conn.id());

    conn.on_message(|id, hub, msg| {
        if let WsMessage::Text(text) = msg {
            println!("[Servidor] ID {} enviou: {}", id, text);
            
            // Formato esperado: TO:ID:MENSAGEM
            if text.starts_with("TO:") {
                let parts: Vec<&str> = text.splitn(3, ':').collect();
                if parts.len() == 3 {
                    if let Ok(target_id) = parts[1].parse::<u64>() {
                        let texto = parts[2];
                        let formatted = format!("Mensagem Privada de {}: {}", id, texto);
                        
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
    });

    conn.on_close(|id, _hub, _code| {
        println!("[Servidor] ID {} saiu.", id);
    });
}

fn main() {
    let port = "8089";
    println!("Iniciando Servidor de Mensagens Privadas na porta {}...", port);
    
    let mut server = Server::new(port);
    server.add_ws_route("/pm", WsMode::Both, handler_pm);
    server.run();
}
