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
use crate::json::{ToJson, FromJson};

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
    pub(crate) stream: TcpStream, // Agora acessivel pelo motor core
    pub(crate) hub: WsHub,
    pub(crate) mode: WsMode,
    pub(crate) config: WsRouteConfig,
    pub(crate) closed: Arc<Mutex<bool>>,
    pub(crate) last_pong: Arc<Mutex<SystemTime>>,
    pub(crate) on_message_cb: Option<Arc<dyn Fn(u64, WsHub, WsMessage) + Send + Sync>>,
    pub(crate) on_close_cb: Option<Arc<dyn Fn(u64, WsHub, Option<u16>) + Send + Sync>>,
    // RFC 6455 Sec 5.4: Estado de fragmentacao
    fragment_buffer: Vec<u8>,
    fragment_opcode: Option<Opcode>,
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
            on_message_cb: None,
            on_close_cb: None,
            fragment_buffer: Vec::new(),
            fragment_opcode: None,
        })
    }

    /// Registra um callback para mensagens recebidas.
    pub fn on_message<F>(&mut self, cb: F)
    where
        F: Fn(u64, WsHub, WsMessage) + Send + Sync + 'static,
    {
        self.on_message_cb = Some(Arc::new(cb));
    }

    /// Registra um callback para mensagens WebSocket tentando fazer parse de JSON automaticamente.
    pub fn on_message_json<T, F>(&mut self, cb: F)
    where
        T: FromJson,
        F: Fn(u64, WsHub, Result<T, String>) + Send + Sync + 'static,
    {
        self.on_message(move |id, hub, msg| {
            if let WsMessage::Text(texto) = msg {
                let parsed = T::from_json(&texto);
                cb(id, hub, parsed);
            }
        });
    }

    /// Registra um callback para quando a conexão for fechada.
    pub fn on_close<F>(&mut self, cb: F)
    where
        F: Fn(u64, WsHub, Option<u16>) + Send + Sync + 'static,
    {
        self.on_close_cb = Some(Arc::new(cb));
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

    /// Envia uma mensagem em formato JSON convertendo a struct diretamente
    pub fn send_json<T: ToJson>(&mut self, data: &T) -> bool {
        self.send(&data.to_json())
    }

    /// Executa um unico ciclo de leitura.
    /// Chamado internamente pelo motor de Epoll do Servidor.
    /// Suporta fragmentacao conforme RFC 6455 Sec 5.4.
    pub(crate) fn process_next_frame(&mut self) -> bool {
        if self.is_closed() || self.mode == WsMode::SendOnly {
            return false;
        }

        let ws_frame = match frame::read_frame(&mut self.stream, self.config.max_message_size, true) {
            Some(f) => f,
            None => {
                // TCP caiu, RSV invalido, mask ausente, ou frame de controle fragmentado
                self.internal_close(Some(1002));
                return false;
            }
        };

        // RFC 6455 Sec 5.4: Frames de controle podem ser intercalados no meio de fragmentacao.
        // Eles DEVEM ser processados imediatamente e NAO afetam o estado de fragmentacao.
        if ws_frame.opcode.is_control() {
            return self.handle_control_frame(ws_frame);
        }

        match ws_frame.opcode {
            Opcode::Text | Opcode::Binary => {
                // Inicio de uma nova mensagem.
                if self.fragment_opcode.is_some() {
                    // Erro: recebemos um novo data frame no meio de uma mensagem fragmentada.
                    self.internal_close(Some(1002));
                    return false;
                }

                if ws_frame.fin {
                    // Mensagem completa em um unico frame (caso mais comum).
                    self.deliver_message(ws_frame.opcode, ws_frame.payload);
                } else {
                    // Primeiro frame de uma mensagem fragmentada.
                    self.fragment_opcode = Some(ws_frame.opcode);
                    self.fragment_buffer = ws_frame.payload;
                }
                true
            }
            Opcode::Continuation => {
                // Frame de continuacao: so e valido se ja estamos acumulando.
                if self.fragment_opcode.is_none() {
                    // Erro: continuation sem um frame inicial.
                    self.internal_close(Some(1002));
                    return false;
                }

                self.fragment_buffer.extend_from_slice(&ws_frame.payload);

                // Protecao contra mensagens fragmentadas gigantes.
                if self.fragment_buffer.len() > self.config.max_message_size {
                    self.internal_close(Some(1009));
                    return false;
                }

                if ws_frame.fin {
                    // Mensagem completa. Pega o opcode original e entrega.
                    let opcode = self.fragment_opcode.take().unwrap();
                    let payload = std::mem::take(&mut self.fragment_buffer);
                    self.deliver_message(opcode, payload);
                }
                true
            }
            Opcode::Unknown(_) => {
                // Opcode desconhecido e um erro de protocolo.
                self.internal_close(Some(1002));
                false
            }
            _ => true,
        }
    }

    /// Processa frames de controle (Close, Ping, Pong) independentemente do estado de fragmentacao.
    fn handle_control_frame(&mut self, ws_frame: frame::WsFrame) -> bool {
        match ws_frame.opcode {
            Opcode::Close => {
                let code = if ws_frame.payload.len() >= 2 {
                    Some(u16::from_be_bytes([ws_frame.payload[0], ws_frame.payload[1]]))
                } else {
                    None
                };
                self.internal_close(code);
                false
            }
            Opcode::Ping => {
                let frame_bytes = frame::encode_frame(Opcode::Pong, &ws_frame.payload);
                let _ = self.stream.write_all(&frame_bytes);
                true
            }
            Opcode::Pong => {
                *self.last_pong.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = SystemTime::now();
                true
            }
            _ => true,
        }
    }

    /// Entrega uma mensagem completa (possivelmente reassemblada de fragmentos) ao callback do usuario.
    fn deliver_message(&self, opcode: Opcode, payload: Vec<u8>) {
        let msg = match opcode {
            Opcode::Text => {
                let text = String::from_utf8_lossy(&payload).to_string();
                WsMessage::Text(text)
            }
            Opcode::Binary => WsMessage::Binary(payload),
            _ => return,
        };
        if let Some(cb) = &self.on_message_cb {
            cb(self.id, self.hub.clone(), msg);
        }
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
        if let Some(f) = frame::read_frame(&mut self.stream, self.config.max_message_size, true) {
            if f.opcode == Opcode::Close {}
        }

        self.internal_close(None);
    }

    pub(crate) fn internal_close(&mut self, code: Option<u16>) {
        let mut c = self.closed.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        if !*c {
            *c = true;
            let _ = self.stream.shutdown(std::net::Shutdown::Both);
            self.hub.unregister(self.id);
            if let Some(cb) = &self.on_close_cb {
                cb(self.id, self.hub.clone(), code);
            }
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
