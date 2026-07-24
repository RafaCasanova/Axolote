extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};
use axolote::ws::{WsConnection, WsMode, WsMessage, WsHub, WsRouteConfig};

/// Handler WebSocket Bidirecional — Chat com Salas e Broadcast
fn handler_chat(conn: &mut WsConnection, hub: WsHub) {
    // 1. Cliente entra na sala "geral" e anuncia sua chegada
    let id = conn.id();
    conn.join("geral");
    conn.set_metadata("username", &format!("User_{}", id));
    
    // Broadcast avisando todos na sala que um novo cliente entrou
    hub.broadcast_to_room("geral", &format!("🔊 O {} entrou no chat!", conn.get_metadata("username").unwrap()));
    
    // Dá boas-vindas específicas só para ele
    conn.send("Bem-vindo ao Chat Global do Chips In Hand! Você está na sala 'geral'.");

    conn.on_message(|id, hub, msg| {
        match msg {
            WsMessage::Text(text) => {
                println!("[WS Chat] [ID:{}] Recebido: {}", id, text);

                // Comandos Especiais
                if text.starts_with("/nome ") {
                    let new_name = text.replace("/nome ", "");
                    hub.set_client_metadata(id, "username", &new_name);
                    hub.send_to(id, &format!("Seu nome agora é {}", new_name));
                    return;
                }
                
                if text == "/online" {
                    hub.send_to(id, &format!("Total de usuários online no servidor: {}", hub.count()));
                    return;
                }

                // Ecoa a mensagem para TODOS na sala, EXCETO ele mesmo
                let username = hub.get_client_metadata(id, "username").unwrap();
                let texto_formatado = format!("[{}] diz: {}", username, text);
                hub.broadcast_to_room_except("geral", id, &texto_formatado);
                
                // Manda de volta pra ele confirmando (apenas pra ele ver)
                hub.send_to(id, &format!("Você: {}", text));
            }
            _ => {}
        }
    });

    conn.on_close(|id, hub, _code| {
        println!("[WS Chat] [ID:{}] Cliente solicitou fechamento ou caiu.", id);
        let username = hub.get_client_metadata(id, "username").unwrap_or_default();
        hub.broadcast_to_room("geral", &format!("🔈 O {} saiu do chat.", username));
    });
}

/// Handler HTTP normal para mostrar uma página de teste
fn handler_pagina_teste(_req: HttpRequest) -> HttpResponse {
    let html = r#"<!DOCTYPE html>
<html>
<head><title>Chips In Hand - Global Chat</title></head>
<body style="font-family:sans-serif; background:#f4f4f4; padding:20px;">
    <h2>Chat Global (WebSocket c/ Broadcast)</h2>
    <div id="log" style="background:#fff; border:1px solid #ccc; padding:16px; height:300px; overflow-y:auto; margin-bottom:10px;"></div>
    
    <input id="msg" type="text" placeholder="Digite uma mensagem ou /nome X..." style="width:300px; padding:8px;">
    <button onclick="enviar()" style="padding:8px 16px;">Enviar</button>
    <button onclick="fechar()" style="padding:8px 16px;">Sair</button>
    
    <script>
        const log = document.getElementById('log');
        const input = document.getElementById('msg');
        const ws = new WebSocket('ws://' + location.host + '/chat');

        ws.onopen = () => addLog('<i>Conectando ao servidor...</i>');
        ws.onmessage = (e) => addLog('<b>' + e.data + '</b>');
        ws.onclose = (e) => addLog('<i>Desconectado. ' + (e.wasClean ? 'Normal' : 'Queda') + '</i>');
        ws.onerror = () => addLog('<i style="color:red">Erro na conexao</i>');

        function addLog(msg) {
            log.innerHTML += '<div>' + msg + '</div>';
            log.scrollTop = log.scrollHeight;
        }

        function enviar() {
            const msg = input.value;
            if (msg && ws.readyState === 1) {
                ws.send(msg);
                input.value = '';
            }
        }

        function fechar() {
            ws.close(1000, 'Tchau');
        }

        input.addEventListener('keypress', (e) => { if(e.key === 'Enter') enviar(); });
    </script>
</body>
</html>"#;

    let mut response = HttpResponse::ok(html);
    response.headers.push(("Content-Type".to_string(), "text/html; charset=utf-8".to_string()));
    response
}

fn main() {
    let mut server = Server::new("8087");

    // Rota HTTP normal (Front-end do Chat)
    server.add_route(HttpMethod::GET, "/", handler_pagina_teste);

    // Rota WebSocket do Chat com configurações avançadas
    server.add_ws_route_with_config("/chat", WsMode::Both, WsRouteConfig {
        max_message_size: 1024 * 1024,
        ping_interval_secs: Some(15),
        pong_timeout_secs: Some(5),
        security: None,
        id_extractor: None,
    }, handler_chat);

    server.run();
}
