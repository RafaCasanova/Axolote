/// Motor de Framing WebSocket (RFC 6455)
/// Encoding e Decoding de frames binários sobre TCP

use std::io::{Read, Write};
use std::net::TcpStream;

/// Opcodes definidos pelo RFC 6455
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Opcode {
    Continuation, // 0x0 — frame de continuacao (fragmentacao)
    Text,         // 0x1
    Binary,       // 0x2
    Close,        // 0x8
    Ping,         // 0x9
    Pong,         // 0xA
    Unknown(u8),
}

impl Opcode {
    pub fn from_byte(b: u8) -> Self {
        match b {
            0x0 => Opcode::Continuation,
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
            Opcode::Continuation => 0x0,
            Opcode::Text => 0x1,
            Opcode::Binary => 0x2,
            Opcode::Close => 0x8,
            Opcode::Ping => 0x9,
            Opcode::Pong => 0xA,
            Opcode::Unknown(b) => *b,
        }
    }

    /// RFC 6455 Sec 5.5: frames de controle tem opcode >= 0x8
    pub fn is_control(&self) -> bool {
        self.to_byte() >= 0x8
    }
}

/// Frame bruto decodificado do WebSocket
pub struct WsFrame {
    pub fin: bool,
    pub rsv: u8,      // RSV1|RSV2|RSV3 (bits 4-6 do primeiro byte)
    pub opcode: Opcode,
    pub payload: Vec<u8>,
}

/// Lê um frame WebSocket completo do TcpStream
/// Retorna None se a conexão foi encerrada, ocorreu um erro, ou o frame excedeu max_size
/// Le um frame WebSocket da stream.
pub fn read_frame(stream: &mut TcpStream, max_size: usize, require_mask: bool) -> Option<WsFrame> {
    // Byte 1: FIN bit + opcode
    let mut header = [0u8; 2];
    if stream.read_exact(&mut header).is_err() {
        return None;
    }

    let fin = (header[0] & 0x80) != 0;
    let rsv = (header[0] >> 4) & 0x07; // bits RSV1, RSV2, RSV3
    let opcode = Opcode::from_byte(header[0] & 0x0F);
    let masked = (header[1] & 0x80) != 0;

    // RFC 6455 Sec 5.2: RSV bits DEVEM ser 0 a menos que uma extensao os defina.
    // Sem extensoes negociadas, qualquer RSV != 0 e um erro de protocolo.
    if rsv != 0 {
        return None;
    }

    // RFC 6455 Sec 5.1: O servidor DEVE fechar a conexao se receber um frame nao mascarado do cliente.
    if require_mask && !masked {
        return None;
    }

    // RFC 6455 Sec 5.5: Frames de controle NAO podem ser fragmentados.
    if opcode.is_control() && !fin {
        return None;
    }
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

    Some(WsFrame { fin, rsv, opcode, payload })
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

/// Escreve um frame mascarado no stream (utilizado por clientes de teste).
pub fn write_client_frame(stream: &mut std::net::TcpStream, opcode: Opcode, payload: &[u8]) {
    use std::io::Write;
    use std::time::SystemTime;
    let mut header = vec![(0x80 | opcode.to_byte())];
    
    let len = payload.len();
    if len <= 125 {
        header.push((len as u8) | 0x80);
    } else if len <= 65535 {
        header.push(126 | 0x80);
        header.extend_from_slice(&(len as u16).to_be_bytes());
    } else {
        header.push(127 | 0x80);
        header.extend_from_slice(&(len as u64).to_be_bytes());
    }

    let mask_key: [u8; 4] = [
        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().subsec_nanos() as u8,
        0x42,
        0x7A,
        0x13
    ];
    header.extend_from_slice(&mask_key);

    let mut masked_payload = Vec::with_capacity(payload.len());
    for (i, &byte) in payload.iter().enumerate() {
        masked_payload.push(byte ^ mask_key[i % 4]);
    }

    if stream.write_all(&header).is_err() { return; }
    let _ = stream.write_all(&masked_payload);
}
