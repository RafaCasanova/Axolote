/// WsConnection — A struct que o usuário manipula dentro do handler WebSocket
/// Agora integrada ao Hub, e com thread de escrita assíncrona otimizada.

use std::net::TcpStream;
use std::sync::{Arc, Mutex};
use std::sync::mpsc;
use std::thread;
use std::time::SystemTime;
use std::io::Write;
use super::frame::{self, Opcode};
use super::hub::WsHub;
use crate::ws::security::WsSecurityGuard;
use crate::json::{FromJson, ToJson};

/// Configurações avançadas da rota WebSocket
#[derive(Clone)]
pub struct WsRouteConfig {
    pub max_message_size: usize,
    pub ping_interval_secs: Option<u64>,
    pub pong_timeout_secs: Option<u64>,
    pub security: Option<Arc<WsSecurityGuard>>,
    pub id_extractor: Option<Arc<dyn Fn(&crate::http::request::HttpRequest) -> Option<u64> + Send + Sync>>,
}

impl Default for WsRouteConfig {
    fn default() -> Self {
        WsRouteConfig {
            max_message_size: 65536, // 64KB padrão
            ping_interval_secs: Some(30), // Heartbeat padrão a cada 30s
            pong_timeout_secs: Some(10),  // 10s para timeout do pong
            security: None,
            id_extractor: None,
        }
    }
}

impl WsRouteConfig {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_max_message_size(mut self, size: usize) -> Self {
        self.max_message_size = size;
        self
    }

    pub fn with_heartbeat(mut self, ping_interval_secs: u64, pong_timeout_secs: u64) -> Self {
        self.ping_interval_secs = Some(ping_interval_secs);
        self.pong_timeout_secs = Some(pong_timeout_secs);
        self
    }

    pub fn with_id_extractor<F>(mut self, extractor: F) -> Self 
    where F: Fn(&crate::http::request::HttpRequest) -> Option<u64> + Send + Sync + 'static {
        self.id_extractor = Some(Arc::new(extractor));
        self
    }

    pub fn id_from_query(mut self, key: &str) -> Self {
        let key_str = key.to_string();
        self.id_extractor = Some(Arc::new(move |req| {
            req.query_params.get(&key_str).and_then(|s| s.parse::<u64>().ok())
        }));
        self
    }

    pub fn id_from_header(mut self, key: &str) -> Self {
        let key_str = key.to_string();
        self.id_extractor = Some(Arc::new(move |req| {
            req.headers.get(&key_str).and_then(|s| s.parse::<u64>().ok())
        }));
        self
    }
}

/// Define a direção permitida na conexão WebSocket
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum WsMode {
    SendOnly,
    ReceiveOnly,
    Both,
}

#[derive(Debug)]
pub enum WsMessage {
    Text(String),
    Binary(Vec<u8>),
    Close(Option<u16>),
    Ping,
    Pong,
}

pub struct WsConnection {
    id: u64,
    stream: TcpStream, // Usado apenas para leitura no handler
    hub: WsHub,
    mode: WsMode,
    config: WsRouteConfig,
    closed: Arc<Mutex<bool>>,
    last_pong: Arc<Mutex<SystemTime>>,
}

impl WsConnection {
    pub(crate) fn new(
        stream: TcpStream, 
        mode: WsMode, 
        hub: WsHub, 
        id: u64, 
        config: WsRouteConfig
    ) -> Option<Self> {
        let closed = Arc::new(Mutex::new(false));
        let last_pong = Arc::new(Mutex::new(SystemTime::now()));
        
        // Feature 5: Zero-Copy Broadcast, enviamos Arc<[u8]>
        let (tx, rx) = mpsc::channel::<Arc<[u8]>>();
        
        // Registra o Sender no Hub, com config do timer (Feature 7)
        hub.register(
            id, 
            tx, 
            Arc::clone(&last_pong), 
            config.ping_interval_secs, 
            config.pong_timeout_secs
        );

        // Clona as coisas para a thread de escrita
        let mut write_stream = match stream.try_clone() {
            Ok(s) => s,
            Err(_) => return None, // Ignora silenciosamente para evitar crash do SO
        };
        let write_closed = Arc::clone(&closed);
        
        // Thread de Escrita (lê do MPSC e escreve no TCP)
        // Feature 6: Otimização de Stack Size (32KB em vez de 2MB)
        let _ = thread::Builder::new()
            .stack_size(32 * 1024)
            .spawn(move || {
                for frame_bytes in rx {
                    if *write_closed.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) {
                        break; // Conexão fechada, para de escrever
                    }
                    if write_stream.write_all(&frame_bytes).is_err() {
                        break;
                    }
                }
            });

        // NOTA (Feature 7): A thread de Ping dedicada foi removida!
        // O Timer Global no Hub cuida dos heartbeats.

        Some(WsConnection {
            id,
            stream,
            hub,
            mode,
            config,
            closed,
            last_pong,
        })
    }

    /// Retorna o ID único desta conexão
    pub fn id(&self) -> u64 {
        self.id
    }

    /// Tenta alterar o ID da conexão (útil para autenticação com IDs do banco de dados).
    /// Retorna true se a alteração foi feita, ou false se o novo ID já estiver em uso.
    pub fn change_id(&mut self, new_id: u64) -> bool {
        if self.hub.change_client_id(self.id, new_id) {
            self.id = new_id;
            true
        } else {
            false
        }
    }

    /// Retorna o modo de direção
    pub fn mode(&self) -> WsMode {
        self.mode
    }

    /// Retorna true se a conexão já foi fechada
    pub fn is_closed(&self) -> bool {
        *self.closed.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// Adiciona a conexão a uma sala no Hub
    pub fn join(&self, room: &str) {
        self.hub.join_room(self.id, room);
    }

    /// Remove a conexão de uma sala no Hub
    pub fn leave(&self, room: &str) {
        self.hub.leave_room(self.id, room);
    }

    /// Define um metadado associado a esta conexão
    pub fn set_metadata(&self, key: &str, value: &str) {
        self.hub.set_client_metadata(self.id, key, value);
    }

    /// Lê um metadado associado a esta conexão
    pub fn get_metadata(&self, key: &str) -> Option<String> {
        self.hub.get_client_metadata(self.id, key)
    }

    /// Envia uma mensagem de texto apenas para este cliente
    pub fn send(&mut self, msg: &str) -> bool {
        if self.is_closed() || self.mode == WsMode::ReceiveOnly {
            return false;
        }
        self.hub.send_to(self.id, msg)
    }

    /// Recebe a próxima mensagem do cliente (bloqueante)
    pub fn receive(&mut self) -> Option<WsMessage> {
        if self.is_closed() || self.mode == WsMode::SendOnly {
            return None;
        }

        loop {
            let ws_frame = match frame::read_frame(&mut self.stream, self.config.max_message_size) {
                Some(f) => f,
                None => {
                    // TCP caiu, excedeu max_size ou erro
                    self.internal_close();
                    return None;
                }
            };

            match ws_frame.opcode {
                Opcode::Text => {
                    let text = String::from_utf8_lossy(&ws_frame.payload).to_string();
                    return Some(WsMessage::Text(text));
                }
                Opcode::Binary => {
                    return Some(WsMessage::Binary(ws_frame.payload));
                }
                Opcode::Close => {
                    let code = if ws_frame.payload.len() >= 2 {
                        Some(u16::from_be_bytes([ws_frame.payload[0], ws_frame.payload[1]]))
                    } else {
                        None
                    };
                    self.internal_close();
                    return Some(WsMessage::Close(code));
                }
                Opcode::Ping => {
                    // Responde com Pong usando a própria thread TCP se possível (ou Hub)
                    let frame_bytes = frame::encode_frame(Opcode::Pong, &ws_frame.payload);
                    let _ = self.stream.write_all(&frame_bytes);
                    return Some(WsMessage::Ping);
                }
                Opcode::Pong => {
                    // Atualiza o relógio do Heartbeat
                    *self.last_pong.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = SystemTime::now();
                    return Some(WsMessage::Pong);
                }
                Opcode::Unknown(_) => {
                    continue;
                }
            }
        }
    }

    /// Envia uma mensagem em formato JSON convertendo a struct diretamente
    pub fn send_json<T: ToJson>(&mut self, data: &T) -> bool {
        self.send(&data.to_json())
    }

    /// Recebe a próxima mensagem do cliente e tenta converter de JSON para a struct T.
    /// Retorna `None` se a conexão foi fechada.
    /// Retorna `Some(Ok(T))` se parseou com sucesso.
    /// Retorna `Some(Err(String))` se a mensagem não era JSON válido.
    pub fn receive_json<T: FromJson>(&mut self) -> Option<Result<T, String>> {
        if let Some(msg) = self.receive() {
            if let WsMessage::Text(text) = msg {
                return Some(T::from_json(&text));
            } else {
                return Some(Err("Mensagem WebSocket não era texto".to_string()));
            }
        }
        None
    }

    /// Inicia o fechamento educado (Close Handshake)
    pub fn close(&mut self) {
        if self.is_closed() {
            return;
        }
        
        let close_bytes = frame::encode_frame(Opcode::Close, &1000u16.to_be_bytes());
        let _ = self.stream.write_all(&close_bytes);

        // Tentamos ler o Close do outro lado
        let _ = self.stream.set_read_timeout(Some(std::time::Duration::from_secs(5)));
        if let Some(f) = frame::read_frame(&mut self.stream, self.config.max_message_size) {
            if f.opcode == Opcode::Close {}
        }

        self.internal_close();
    }

    fn internal_close(&mut self) {
        let mut c = self.closed.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*c {
            *c = true;
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            self.hub.unregister(self.id);
        }
    }
}

impl Drop for WsConnection {
    fn drop(&mut self) {
        if !self.is_closed() {
            self.close();
        }
    }
}
