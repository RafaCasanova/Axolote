/// Protocolo de Envelope S2S (Server-to-Server)
/// Define o formato binario das mensagens trafegadas entre nos do cluster.
/// 
/// Layout do envelope:
/// [1 byte]  msg_type      - Tipo da mensagem (Broadcast, Room, SendTo, Presence, Heartbeat)
/// [1 byte]  node_origin   - ID do no que originou a mensagem
/// [8 bytes] message_seq   - Contador sequencial atomico do no de origem
/// [2 bytes] target_len    - Tamanho do campo target
/// [N bytes] target        - Nome da sala (UTF-8) ou ID do usuario (u64 big-endian, 8 bytes)
/// [4 bytes] payload_len   - Tamanho do payload
/// [M bytes] payload       - Dados brutos (frame WS codificado ou dados de controle)

/// Tipos de mensagem no protocolo S2S
#[derive(Debug, Clone, Copy, PartialEq)]
#[repr(u8)]
pub enum S2sMessageType {
    /// Broadcast global para todos os clientes de todos os nos
    Broadcast = 0,
    /// Broadcast para uma sala especifica
    BroadcastRoom = 1,
    /// Mensagem direta para um ID de usuario especifico
    SendTo = 2,
    /// Notificacao de presenca: um usuario conectou ou desconectou
    PresenceUpdate = 3,
    /// Heartbeat entre servidores (keepalive S2S)
    Heartbeat = 4,
    /// Broadcast global excluindo um ID
    BroadcastExcept = 5,
    /// Broadcast para sala excluindo um ID
    BroadcastRoomExcept = 6,
}

impl S2sMessageType {
    pub fn from_byte(b: u8) -> Option<Self> {
        match b {
            0 => Some(S2sMessageType::Broadcast),
            1 => Some(S2sMessageType::BroadcastRoom),
            2 => Some(S2sMessageType::SendTo),
            3 => Some(S2sMessageType::PresenceUpdate),
            4 => Some(S2sMessageType::Heartbeat),
            5 => Some(S2sMessageType::BroadcastExcept),
            6 => Some(S2sMessageType::BroadcastRoomExcept),
            _ => None,
        }
    }
}

/// Envelope S2S decodificado
#[derive(Debug, Clone)]
pub struct S2sEnvelope {
    pub msg_type: S2sMessageType,
    pub node_origin: u8,
    pub message_seq: u64,
    pub target: Vec<u8>,
    pub payload: Vec<u8>,
}

impl S2sEnvelope {
    /// Serializa o envelope em bytes para envio via TCP
    pub fn encode(&self, secret: Option<&[u8]>) -> Vec<u8> {
        let target_len = self.target.len() as u16;
        let payload_len = self.payload.len() as u32;

        let total_size = 1 + 1 + 8 + 2 + self.target.len() + 4 + self.payload.len();
        let mut buf = Vec::with_capacity(total_size);

        buf.push(self.msg_type as u8);
        buf.push(self.node_origin);
        buf.extend_from_slice(&self.message_seq.to_be_bytes());
        buf.extend_from_slice(&target_len.to_be_bytes());
        buf.extend_from_slice(&self.target);
        buf.extend_from_slice(&payload_len.to_be_bytes());
        buf.extend_from_slice(&self.payload);

        if let Some(key) = secret {
            let mac = crate::ws::crypto::hmac_sha1(key, &buf);
            buf.extend_from_slice(&mac);
        }

        buf
    }

    /// Deserializa bytes em um envelope S2S.
    /// Retorna None se os dados estiverem corrompidos, incompletos ou a assinatura falhar.
    pub fn decode(data: &[u8], secret: Option<&[u8]>) -> Option<Self> {
        let (actual_data, _signature_len) = if let Some(key) = secret {
            if data.len() < 20 { return None; }
            let msg_len = data.len() - 20;
            let expected_mac = crate::ws::crypto::hmac_sha1(key, &data[..msg_len]);
            let received_mac = &data[msg_len..];
            
            let mut diff = 0;
            for (x, y) in expected_mac.iter().zip(received_mac.iter()) {
                diff |= x ^ y;
            }
            if diff != 0 {
                return None; // Assinatura invalida (Spoofing detectado)
            }
            (&data[..msg_len], 20)
        } else {
            (data, 0)
        };

        if actual_data.len() < 12 {
            return None; // Minimo: 1+1+8+2 = 12 bytes de header
        }

        let msg_type = S2sMessageType::from_byte(actual_data[0])?;
        let node_origin = actual_data[1];
        let message_seq = u64::from_be_bytes([
            actual_data[2], actual_data[3], actual_data[4], actual_data[5],
            actual_data[6], actual_data[7], actual_data[8], actual_data[9],
        ]);
        let target_len = u16::from_be_bytes([actual_data[10], actual_data[11]]) as usize;

        let target_end = 12 + target_len;
        if actual_data.len() < target_end + 4 {
            return None;
        }

        let target = actual_data[12..target_end].to_vec();

        let payload_len = u32::from_be_bytes([
            actual_data[target_end],
            actual_data[target_end + 1],
            actual_data[target_end + 2],
            actual_data[target_end + 3],
        ]) as usize;

        let payload_start = target_end + 4;
        let payload_end = payload_start + payload_len;
        if actual_data.len() < payload_end {
            return None;
        }

        let payload = actual_data[payload_start..payload_end].to_vec();

        Some(S2sEnvelope {
            msg_type,
            node_origin,
            message_seq,
            target,
            payload,
        })
    }

    /// Retorna o tamanho total do envelope serializado
    pub fn encoded_size(&self, secret: Option<&[u8]>) -> usize {
        let base = 1 + 1 + 8 + 2 + self.target.len() + 4 + self.payload.len();
        if secret.is_some() { base + 20 } else { base }
    }
}

/// Le um envelope completo de um TcpStream usando length-prefixed framing.
/// Formato no fio: [4 bytes: tamanho total do envelope][N bytes: envelope]
pub fn read_envelope<R: std::io::Read>(reader: &mut R, secret: Option<&[u8]>) -> Option<S2sEnvelope> {
    let mut len_buf = [0u8; 4];
    if reader.read_exact(&mut len_buf).is_err() {
        return None;
    }
    let total_len = u32::from_be_bytes(len_buf) as usize;

    if total_len > 1_048_576 {
        // Limite de seguranca: 1MB por envelope S2S
        return None;
    }

    let mut data = vec![0u8; total_len];
    if reader.read_exact(&mut data).is_err() {
        return None;
    }

    S2sEnvelope::decode(&data, secret)
}

/// Escreve um envelope em um TcpStream com length-prefix.
pub fn write_envelope<W: std::io::Write>(writer: &mut W, envelope: &S2sEnvelope, secret: Option<&[u8]>) -> bool {
    let encoded = envelope.encode(secret);
    let len_bytes = (encoded.len() as u32).to_be_bytes();

    if writer.write_all(&len_bytes).is_err() {
        return false;
    }
    if writer.write_all(&encoded).is_err() {
        return false;
    }
    true
}
