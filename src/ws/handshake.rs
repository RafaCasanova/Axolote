/// Handshake HTTP → WebSocket (RFC 6455 Section 4.2.2)
/// Detecta o pedido de Upgrade e responde com HTTP 101

use std::collections::HashMap;
use std::io::Write;
use std::net::TcpStream;
use super::crypto::{sha1, base64_encode};

/// A string mágica definida pelo RFC 6455 para o cálculo do Accept
const WS_MAGIC: &str = "258EAFA5-E914-47DA-95CA-5AB5DC786C11";

/// Verifica se os headers HTTP indicam um pedido de Upgrade para WebSocket
pub fn is_websocket_upgrade(headers: &HashMap<String, String>) -> bool {
    let upgrade = headers.get("Upgrade")
        .or_else(|| headers.get("upgrade"))
        .map(|v| v.to_lowercase());
    
    let connection = headers.get("Connection")
        .or_else(|| headers.get("connection"))
        .map(|v| v.to_lowercase());

    upgrade.as_deref() == Some("websocket")
        && connection.map_or(false, |c| c.contains("upgrade"))
}

/// Extrai o Sec-WebSocket-Key dos headers
pub fn get_ws_key(headers: &HashMap<String, String>) -> Option<String> {
    headers.get("Sec-WebSocket-Key")
        .or_else(|| headers.get("sec-websocket-key"))
        .cloned()
}

/// Calcula o valor do Sec-WebSocket-Accept a partir da chave do cliente
/// Accept = Base64(SHA-1(key + magic_string))
pub fn compute_accept_key(client_key: &str) -> String {
    let mut input = client_key.to_string();
    input.push_str(WS_MAGIC);
    let hash = sha1(input.as_bytes());
    base64_encode(&hash)
}

/// Envia a resposta HTTP 101 Switching Protocols para completar o handshake
pub fn send_upgrade_response(stream: &mut TcpStream, accept_key: &str, protocol: Option<&str>) -> bool {
    let mut response = format!(
        "HTTP/1.1 101 Switching Protocols\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Accept: {}\r\n",
        accept_key
    );

    if let Some(proto) = protocol {
        response.push_str(&format!("Sec-WebSocket-Protocol: {}\r\n", proto));
    }

    response.push_str("\r\n");

    stream.write_all(response.as_bytes()).is_ok()
}
