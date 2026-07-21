extern crate axolote;
use axolote::prelude::*;

fn private_chat_handler(mut conn: WsConnection, hub: WsHub) {
    // 1. O cliente se conecta com um ID temporario sequencial gerado pelo framework.
    // O cliente envia uma mensagem inicial para autenticacao contendo o ID do seu banco de dados.
    // Formato esperado da primeira mensagem: "AUTH:USER_DB_ID"
    
    let mut authenticated = false;

    loop {
        match conn.receive() {
            Some(WsMessage::Text(texto)) => {
                if !authenticated {
                    if texto.starts_with("AUTH:") {
                        let parts: Vec<&str> = texto.split(':').collect();
                        if parts.len() == 2 {
                            if let Ok(db_id) = parts[1].parse::<u64>() {
                                // Tenta mudar o ID temporario da conexao para o ID do banco do usuario no Hub
                                if conn.change_id(db_id) {
                                    conn.send("AUTH_SUCCESS");
                                    authenticated = true;
                                } else {
                                    conn.send("AUTH_FAILED: ID ja conectado.");
                                    conn.close();
                                    break;
                                }
                            }
                        }
                    }
                    if !authenticated {
                        conn.send("AUTH_REQUIRED");
                        conn.close();
                        break;
                    }
                } else {
                    // 2. Apos a autenticacao, processamos as mensagens normais
                    // Formato esperado de mensagem privada: "TO:DESTINATARIO_ID:MENSAGEM"
                    if texto.starts_with("TO:") {
                        let parts: Vec<&str> = texto.splitn(3, ':').collect();
                        if parts.len() == 3 {
                            if let Ok(dest_id) = parts[1].parse::<u64>() {
                                let conteudo = parts[2];
                                let remetente = conn.id();
                                let formatada = format!("Mensagem Privada de {}: {}", remetente, conteudo);
                                
                                // Envia diretamente para o ID do destinatario no Hub
                                if hub.send_to(dest_id, &formatada) {
                                    conn.send("ENVIO_OK");
                                } else {
                                    conn.send("ENVIO_FALHA: Usuario offline.");
                                }
                            }
                        }
                    }
                }
            }
            Some(WsMessage::Close(_)) | None => break,
            _ => {}
        }
    }
}

fn main() {
    let mut server = Server::new("8080");

    // Adiciona a rota configurando limites de heartbeat
    let ws_config = WsRouteConfig {
        max_message_size: 1024 * 1024,
        ping_interval_secs: Some(30),
        pong_timeout_secs: Some(10),
        security: None,
    };

    server.add_ws_route_with_config("/mensageiro", WsMode::Both, ws_config, private_chat_handler);

    server.run();
}
