/// Motor de Framing WebSocket (RFC 6455)
/// Encoding e Decoding de frames binários sobre TCP

use std::io::{Read, Write};
use std::net::TcpStream;

/// Opcodes definidos pelo RFC 6455
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opcode {
    Text,       // 0x1
    Binary,     // 0x2
    Close,      // 0x8
    Ping,       // 0x9
    Pong,       // 0xA
    Unknown(u8),
}

impl Opcode {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x1 => Opcode::Text,
            0x2 => Opcode::Binary,
            0x8 => Opcode::Close,
            0x9 => Opcode::Ping,
            0xA => Opcode::Pong,
            other => Opcode::Unknown(other),
        }
    }

    pub fn to_byte(&self) -> u8 {
        match self {
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
            Opcode::Unknown(b) => *b,
        }
    }
}

/// Frame bruto decodificado do WebSocket
pub struct WsFrame {
    pub fin: bool,
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

/// Lê um frame WebSocket completo do TcpStream
/// Retorna None se a conexão foi encerrada, ocorreu um erro, ou o frame excedeu max_size
pub fn read_frame(stream: &mut TcpStream, max_size: usize) -> Option<WsFrame> {
    // Byte 1: FIN bit + opcode
    let mut header = [0u8; 2];
    if stream.read_exact(&mut header).is_err() {
        return None;
    }

    let fin = (header[0] & 0x80) != 0;
    let opcode = Opcode::from_byte(header[0] & 0x0F);
    let masked = (header[1] & 0x80) != 0;
    let mut payload_len = (header[1] & 0x7F) as u64;

    // Extended payload length
    if payload_len == 126 {
        let mut buf = [0u8; 2];
        if stream.read_exact(&mut buf).is_err() {
            return None;
        }
        payload_len = u16::from_be_bytes(buf) as u64;
    } else if payload_len == 127 {
        let mut buf = [0u8; 8];
        if stream.read_exact(&mut buf).is_err() {
            return None;
        }
        payload_len = u64::from_be_bytes(buf);
    }

    // Proteção contra frames gigantes (Feature 4: Max Message Size)
    if payload_len > max_size as u64 {
        return None;
    }

    // Masking key (4 bytes, somente se masked == true)
    let mask_key = if masked {
        let mut key = [0u8; 4];
        if stream.read_exact(&mut key).is_err() {
            return None;
        }
        Some(key)
    } else {
        None
    };

    // Payload data
    let mut payload = vec![0u8; payload_len as usize];
    if !payload.is_empty() {
        if stream.read_exact(&mut payload).is_err() {
            return None;
        }
    }

    // Unmask payload (XOR com a chave rotativa de 4 bytes)
    if let Some(key) = mask_key {
        for i in 0..payload.len() {
            payload[i] ^= key[i % 4];
        }
    }

    Some(WsFrame { fin, opcode, payload })
}

/// Escreve um frame WebSocket no TcpStream (server → client, sem mask)
pub fn write_frame(stream: &mut TcpStream, opcode: Opcode, payload: &[u8]) -> bool {
    let mut frame = Vec::new();

    // Byte 1: FIN=1 + opcode
    frame.push(0x80 | opcode.to_byte());

    // Byte 2+: payload length (sem mask, pois é server → client)
    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    // Payload
    frame.extend_from_slice(payload);

    stream.write_all(&frame).is_ok()
}

/// Serializa um frame WebSocket em bytes (para envio via canal MPSC)
pub fn encode_frame(opcode: Opcode, payload: &[u8]) -> Vec<u8> {
    let mut frame = Vec::new();

    frame.push(0x80 | opcode.to_byte());

    let len = payload.len();
    if len < 126 {
        frame.push(len as u8);
    } else if len <= 65535 {
        frame.push(126);
        frame.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        frame.push(127);
        frame.extend_from_slice(&(len as u64).to_be_bytes());
    }

    frame.extend_from_slice(payload);
    frame
}
