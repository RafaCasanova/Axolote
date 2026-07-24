pub mod http;
pub mod logger;
pub mod reactor;
pub mod route;
pub mod route_group;
pub mod thread_pool;
pub mod ws;
pub mod json;

extern crate axolote_macros;

pub use axolote_macros::axolote_json;

/// Módulo Prelude para facilitar a importação em projetos externos
pub mod prelude {
    pub use crate::Server;
    pub use crate::http::{HttpMethod, HttpRequest, HttpResponse};
    pub use crate::route_group::RouteGroup;
    pub use crate::logger::{Logger, LoggerConfig, LogFormat, LogTarget, LogLevel, LogDispatcher};
    pub use crate::json::ToJson;
    pub use crate::ws::{WsConnection, WsMode, WsMessage, WsHandlerFn, WsHub, WsRouteConfig};
    pub use crate::ws::cluster::ClusterConfig; // NEW
}

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::os::unix::io::AsRawFd;
use crate::reactor::{Reactor, EPOLLIN, EPOLLONESHOT};

use self::http::{HttpMethod, HttpRequest, HttpResponse};
use self::route::{HandlerFn, Route};
use self::route_group::RouteGroup;
use self::ws::{WsMode, WsHandlerFn, WsConnection, WsHub, WsRouteConfig};
use self::ws::cluster::{ClusterConfig, ClusterState, ClusterManager};

/// Handler padrão caso nenhuma rota seja encontrada
fn default_not_found_handler(_req: HttpRequest) -> HttpResponse {
    HttpResponse::not_found("404 - Rota nao encontrada")
}

/// Estrutura que armazena uma rota WebSocket
struct WsRoute {
    path: String,
    mode: WsMode,
    config: WsRouteConfig,
    handler: WsHandlerFn,
}

/// Estrutura principal do Servidor HTTP
pub struct Server {
    address: String,
    routes: std::collections::HashMap<HttpMethod, Vec<Route>>,
    groups: Vec<RouteGroup>,
    ws_routes: Vec<WsRoute>,
    ws_hub: WsHub,
    cluster_config: Option<ClusterConfig>, // NEW
    pub cors_config: Option<crate::http::cors::CorsConfig>,
    static_dirs: Vec<(String, String)>,
    not_found_handler: HandlerFn,
    pub logger: crate::logger::Logger,
    pub timeout: std::time::Duration,
    reactor: Arc<Reactor>,
    thread_pool: Option<Arc<crate::thread_pool::ThreadPool>>,
}

impl Server {
    /// Cria um novo servidor HTTP.
    /// `addr` pode ser apenas a porta (ex: "8080" -> bind em 127.0.0.1) 
    /// ou o host completo (ex: "0.0.0.0:8080" -> bind em todas as interfaces).
    pub fn new(addr: &str) -> Self {
        let address = if addr.contains(':') {
            addr.to_string()
        } else {
            format!("127.0.0.1:{}", addr)
        };

        Server {
            address,
            routes: std::collections::HashMap::new(),
            groups: Vec::new(),
            ws_routes: Vec::new(),
            ws_hub: WsHub::new(),
            cluster_config: None, // NEW
            cors_config: None,
            static_dirs: Vec::new(),
            not_found_handler: Box::new(default_not_found_handler),
            logger: crate::logger::Logger::new(crate::logger::LoggerConfig::new(vec![
                    crate::logger::LogDispatcher {
                        min_level: crate::logger::LogLevel::Info,
                        format: crate::logger::LogFormat::Text,
                        target: crate::logger::LogTarget::Console,
                    }
                ]
            )),
            timeout: std::time::Duration::from_secs(10),
            reactor: Arc::new(Reactor::new().unwrap()),
            thread_pool: None,
        }
    }

    /// Cria um novo servidor HTTP escutando em IPv6 [::1] (fallback caso passe so a porta)
    pub fn new_ipv6(addr: &str) -> Self {
        let address = if addr.contains(':') && addr.contains(']') {
            addr.to_string()
        } else {
            format!("[::1]:{}", addr)
        };

        Server {
            address,
            routes: std::collections::HashMap::new(),
            groups: Vec::new(),
            ws_routes: Vec::new(),
            ws_hub: WsHub::new(),
            cluster_config: None, // NEW
            cors_config: None,
            static_dirs: Vec::new(),
            not_found_handler: Box::new(default_not_found_handler),
            logger: crate::logger::Logger::new(crate::logger::LoggerConfig::new(vec![
                    crate::logger::LogDispatcher {
                        min_level: crate::logger::LogLevel::Info,
                        format: crate::logger::LogFormat::Text,
                        target: crate::logger::LogTarget::Console,
                    }
                ]
            )),
            timeout: std::time::Duration::from_secs(10),
            reactor: Arc::new(Reactor::new().unwrap()),
            thread_pool: None,
        }
    }

    /// Habilita o modo cluster distribuido
    pub fn enable_cluster(&mut self, config: ClusterConfig) {
        self.cluster_config = Some(config);
    }

    /// Altera o endereço (host) onde o servidor escutará as conexões.
    /// Para expor na internet ou rede local, use "0.0.0.0".
    pub fn set_host(&mut self, host: &str) {
        if let Some(port_idx) = self.address.rfind(':') {
            let port = &self.address[port_idx..];
            self.address = format!("{}{}", host, port);
        }
    }

    /// Habilita o Middleware nativo de CORS com a configuração fornecida
    pub fn enable_cors(&mut self, config: crate::http::cors::CorsConfig) {
        self.cors_config = Some(config);
    }

    /// Mapeia todas as requisições iniciadas com `route_prefix` para arquivos reais no diretório físico `base_dir`.
    /// Protegido contra Path Traversal e resolve Mime Types automaticamente.
    pub fn serve_dir(&mut self, route_prefix: &str, base_dir: &str) {
        self.static_dirs.push((route_prefix.to_string(), base_dir.to_string()));
    }

    /// Define o timeout de leitura e escrita para as conexões TCP em segundos (padrão: 10s)
    pub fn set_timeout(&mut self, seconds: u64) {
        self.timeout = std::time::Duration::from_secs(seconds);
    }

    /// Define um handler customizado para rotas 404 (Not Found)
    pub fn set_not_found_handler<F>(&mut self, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        self.not_found_handler = Box::new(handler);
    }


    /// Adiciona uma rota avulsa ao servidor (sem grupo/middleware)
    pub fn add_route<F>(&mut self, method: HttpMethod, path: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        self.routes.entry(method.clone()).or_insert_with(Vec::new).push(Route::new(method, path, handler));
    }

    /// Adiciona um grupo de rotas ao servidor
    pub fn add_group(&mut self, group: RouteGroup) {
        self.groups.push(group);
    }

    /// Adiciona uma rota WebSocket ao servidor (com configuração padrão)
    pub fn add_ws_route(&mut self, path: &str, mode: WsMode, handler: WsHandlerFn) {
        self.ws_routes.push(WsRoute {
            path: path.to_string(),
            mode,
            config: WsRouteConfig::default(),
            handler,
        });
    }

    /// Adiciona uma rota WebSocket especificando configurações (max_size, ping)
    pub fn add_ws_route_with_config(&mut self, path: &str, mode: WsMode, config: WsRouteConfig, handler: WsHandlerFn) {
        self.ws_routes.push(WsRoute {
            path: path.to_string(),
            mode,
            config,
            handler,
        });
    }

    /// Roda o servidor e começa a aceitar conexões (multi-threaded)
    pub fn run(mut self) {
        let listener = match TcpListener::bind(&self.address) {
            Ok(l) => l,
            Err(e) => {
                self.logger.error(&format!("Falha ao abrir porta {}: {}", self.address, e));
                return;
            }
        };

        // NEW: Inicia o cluster manager se estiver habilitado
        if let Some(cfg) = self.cluster_config.clone() {
            self.logger.info(&format!("Modo Cluster ativado: Node {} (S2S: {})", cfg.node_id, cfg.s2s_port));
            let state = ClusterState::new(cfg.node_id, cfg.cluster_secret.clone());
            self.ws_hub.enable_cluster(state.clone());
            ClusterManager::start(cfg, state, self.ws_hub.clone());
        } else {
            self.logger.info("Modo Cluster desativado (Standalone)");
        }

        self.logger.info(&format!("Servidor rodando em http://{}", self.address));

        let mut self_mut = self;
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4) * 4;
            
        self_mut.logger.info(&format!("Inicializando ThreadPool com {} threads.", num_threads));
        let pool = Arc::new(crate::thread_pool::ThreadPool::new(num_threads));
        self_mut.thread_pool = Some(Arc::clone(&pool));
        
        let server_arc = Arc::new(self_mut);

        // Lança a thread do Reactor
        let reactor_clone = Arc::clone(&server_arc.reactor);
        std::thread::Builder::new().name("epoll-reactor".to_string()).spawn(move || {
            loop {
                let _ = reactor_clone.poll(100);
            }
        }).unwrap();

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let srv = Arc::clone(&server_arc);
                    let mut fallback_stream = stream.try_clone().unwrap();
                    if let Err(_) = pool.execute(move || {
                        srv.handle_connection(stream);
                    }) {
                        use std::io::Write;
                        let _ = fallback_stream.write_all(b"HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\n\r\n");
                    }
                }
                Err(e) => {
                    server_arc.logger.error(&format!("Erro ao aceitar conexão: {}", e));
                }
            }
        }
    }

    fn handle_connection(&self, mut stream: TcpStream) {
        // Prevenção contra Slowloris: Timeout configurável de leitura e escrita
        let _ = stream.set_read_timeout(Some(self.timeout));
        let _ = stream.set_write_timeout(Some(self.timeout));

        let mut raw_data = Vec::new();
        let mut buffer = [0; 4096];
        const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB
        const MAX_HEADER_SIZE: usize = 64 * 1024; // 64 KB

        'keep_alive: loop {
            let mut headers_ended = false;
            let mut content_length = 0;
            let mut headers_len = 0;

            loop {
                // Primeiro verifica se o `raw_data` atual já tem um header
                if !headers_ended {
                    let mut end_idx = None;
                    for i in 0..raw_data.len().saturating_sub(3) {
                        if &raw_data[i..i+4] == b"\r\n\r\n" {
                            end_idx = Some(i + 4);
                            break;
                        }
                    }
                    if end_idx.is_none() {
                        for i in 0..raw_data.len().saturating_sub(1) {
                            if &raw_data[i..i+2] == b"\n\n" {
                                end_idx = Some(i + 2);
                                break;
                            }
                        }
                    }

                    if let Some(idx) = end_idx {
                        headers_ended = true;
                        headers_len = idx;
                        
                        let header_str = String::from_utf8_lossy(&raw_data[..idx]);
                        for line in header_str.lines() {
                            let line_lower = line.to_lowercase();
                            if line_lower.starts_with("content-length:") {
                                if let Some(parts) = line_lower.split_once(':') {
                                    if let Ok(cl) = parts.1.trim().parse::<usize>() {
                                        content_length = cl;
                                    }
                                }
                            }
                        }
                        if content_length > MAX_BODY_SIZE {
                            self.logger.error("Content-Length excede limite de 10MB");
                            let response = HttpResponse::bad_request("413 - Payload Too Large");
                            let _ = stream.write_all(&response.to_bytes());
                            return;
                        }
                    } else if raw_data.len() > MAX_HEADER_SIZE {
                        self.logger.error("Cabeçalhos excedem limite de 64KB");
                        let _ = stream.write_all(b"HTTP/1.1 431 Request Header Fields Too Large\r\nConnection: close\r\n\r\n");
                        return;
                    }
                }

                if headers_ended {
                    let body_read = raw_data.len() - headers_len;
                    if body_read >= content_length {
                        break; // Terminou de ler toda a request
                    }
                }

                // Lê mais bytes da rede se a request ainda não está completa
                match stream.read(&mut buffer) {
                    Ok(0) => {
                        if raw_data.is_empty() {
                            return; // EOF Limpo
                        }
                        break;
                    },
                    Ok(n) => {
                        raw_data.extend_from_slice(&buffer[..n]);
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock || e.kind() == std::io::ErrorKind::TimedOut => {
                        break;
                    }
                    Err(e) => {
                        self.logger.error(&format!("Erro ao ler stream: {}", e));
                        return;
                    }
                }
            }

            if raw_data.is_empty() {
                // Nada foi lido
                break 'keep_alive;
            }

            let request_len = headers_len + content_length;
            let current_request_data = if raw_data.len() >= request_len {
                &raw_data[..request_len]
            } else {
                &raw_data[..]
            };

            let mut should_close = false;

            if let Some(mut req) = HttpRequest::from_bytes(current_request_data) {
                // Checa se o client quer Connection: close
                let client_wants_close = req.headers.get("connection").map(|v| v.to_lowercase()) == Some("close".to_string());
                
                // Verifica se é um pedido de Upgrade para WebSocket
                if ws::handshake::is_websocket_upgrade(&req.headers) {
                    self.handle_websocket(stream, &req);
                    return;
                }

                // Tratamento CORS nativo (Preflight)
                if req.method == HttpMethod::OPTIONS && self.cors_config.is_some() {
                    if req.headers.contains_key("access-control-request-method") {
                        let response = self.cors_config.as_ref().unwrap().handle_preflight(&req);
                        self.logger.info(&format!(
                            "OPTIONS {} - {} {} (CORS Preflight)",
                            req.path, response.status_code, response.status_text
                        ));
                        let _ = stream.write_all(&response.to_bytes());
                        let _ = stream.flush();
                        if client_wants_close {
                            break 'keep_alive;
                        }
                        if raw_data.len() >= request_len {
                            raw_data = raw_data[request_len..].to_vec();
                        }
                        continue 'keep_alive;
                    }
                }

                let mut response = self.resolve_request(&mut req);

                // Injeta cabeçalhos CORS na resposta se ativado
                if let Some(cors) = &self.cors_config {
                    response = cors.apply_to_response(&req, response);
                }

                // Determina fechamento de conexão
                if client_wants_close {
                    response = response.with_header("Connection", "close");
                    should_close = true;
                } else {
                    response = response.with_header("Connection", "keep-alive");
                }

                self.logger.info(&format!(
                    "{:?} {} - {} {}",
                    req.method, req.path, response.status_code, response.status_text
                ));
                let _ = stream.write_all(&response.to_bytes());
            } else {
                let response = HttpResponse::bad_request("400 - Requisicao invalida").with_header("Connection", "close");
                self.logger.warn("Requisição inválida recebida");
                let _ = stream.write_all(&response.to_bytes());
                should_close = true;
            }
            
            let _ = stream.flush();

            if should_close || raw_data.len() < request_len {
                break 'keep_alive;
            }
            
            raw_data = raw_data[request_len..].to_vec();
        }
    }

    /// Processa uma conexão WebSocket após detectar o header Upgrade
    fn handle_websocket(&self, mut stream: TcpStream, req: &HttpRequest) {
        let ws_route = self.ws_routes.iter().find(|r| r.path == req.path);

        let ws_route = match ws_route {
            Some(r) => r,
            None => {
                self.logger.warn(&format!("WS: Rota não encontrada para {}", req.path));
                let response = HttpResponse::not_found("404 - Rota WebSocket nao encontrada");
                let _ = stream.write_all(&response.to_bytes());
                return;
            }
        };

        let ws_key = match ws::handshake::get_ws_key(&req.headers) {
            Some(key) => key,
            None => {
                self.logger.warn("WS: Sec-WebSocket-Key ausente");
                let response = HttpResponse::bad_request("400 - Sec-WebSocket-Key ausente");
                let _ = stream.write_all(&response.to_bytes());
                return;
            }
        };

        let mut negotiated_protocol: Option<String> = None;

        if let Some(guard) = &ws_route.config.security {
            if guard.strict_rfc {
                if req.method != crate::http::HttpMethod::GET {
                    self.logger.warn("WS Security: Método não é GET");
                    let _ = stream.write_all(&HttpResponse::bad_request("400 - Method Must Be GET").to_bytes());
                    return;
                }
                let version = req.headers.get("sec-websocket-version").or_else(|| req.headers.get("Sec-WebSocket-Version")).map(|s| s.as_str());
                if version != Some("13") {
                    self.logger.warn("WS Security: sec-websocket-version inválido");
                    let _ = stream.write_all(&HttpResponse::new(400, "Bad Request", "400 - Unsupported Version").to_bytes());
                    return;
                }
            }

            if let Some(allowed_origins) = &guard.allowed_origins {
                let origin = req.headers.get("origin").or_else(|| req.headers.get("Origin"));
                if let Some(o) = origin {
                    if !allowed_origins.contains(o) {
                        self.logger.warn(&format!("WS Security: Origem bloqueada: {}", o));
                        let _ = stream.write_all(&HttpResponse::new(403, "Forbidden", "403 - Forbidden Origin").to_bytes());
                        return;
                    }
                } else {
                    self.logger.warn("WS Security: Origem ausente");
                    let _ = stream.write_all(&HttpResponse::new(403, "Forbidden", "403 - Missing Origin").to_bytes());
                    return;
                }
            }

            if let Some(allowed_protos) = &guard.allowed_subprotocols {
                let client_protos = req.headers.get("sec-websocket-protocol").or_else(|| req.headers.get("Sec-WebSocket-Protocol"));
                if let Some(cp) = client_protos {
                    let mut matched = false;
                    for p in cp.split(',') {
                        let p = p.trim();
                        if allowed_protos.iter().any(|ap| ap == p) {
                            negotiated_protocol = Some(p.to_string());
                            matched = true;
                            break;
                        }
                    }
                    if !matched {
                        self.logger.warn("WS Security: Subprotocolo recusado");
                        let _ = stream.write_all(&HttpResponse::bad_request("400 - Unsupported Subprotocol").to_bytes());
                        return;
                    }
                }
            }

            if let Some(validator) = &guard.custom_validator {
                if !validator(req) {
                    self.logger.warn("WS Security: Custom Validator recusou a conexão");
                    let _ = stream.write_all(&HttpResponse::new(403, "Forbidden", "403 - Forbidden").to_bytes());
                    return;
                }
            }

            // Verifica Token na Query String
            for (param, expected) in &guard.required_query_tokens {
                let actual = req.query_params.get(param).map(|s| s.as_str());
                if !actual.map_or(false, |a| crate::ws::crypto::constant_time_eq(a, expected)) {
                    self.logger.warn(&format!("WS Security: Falha na validação do Query Token '{}'", param));
                    let _ = stream.write_all(&HttpResponse::new(401, "Unauthorized", "401 - Unauthorized").to_bytes());
                    return;
                }
            }

            // Verifica Token no Header
            for (header_name, expected) in &guard.required_header_tokens {
                let actual = req.headers.get(header_name)
                    .or_else(|| req.headers.get(&header_name.to_lowercase()))
                    .map(|s| s.as_str());
                if !actual.map_or(false, |a| crate::ws::crypto::constant_time_eq(a, expected)) {
                    self.logger.warn(&format!("WS Security: Falha na validação do Header Token '{}'", header_name));
                    let _ = stream.write_all(&HttpResponse::new(401, "Unauthorized", "401 - Unauthorized").to_bytes());
                    return;
                }
            }
        }

        let accept_key = ws::handshake::compute_accept_key(&ws_key);
        if !ws::handshake::send_upgrade_response(&mut stream, &accept_key, negotiated_protocol.as_deref()) {
            self.logger.error("WS: Falha ao enviar resposta de Upgrade");
            return;
        }

        let mode = ws_route.mode;
        let config = ws_route.config.clone();
        let handler = ws_route.handler;
        
        let mut id = ws::hub::next_connection_id();
        
        if let Some(extractor) = &config.id_extractor {
            if let Some(extracted_id) = extractor(req) {
                if self.ws_hub.is_connected(extracted_id) {
                    self.logger.warn(&format!("WS: Conexão abortada. ID {} já está em uso.", extracted_id));
                    let _ = stream.write_all(&HttpResponse::new(409, "Conflict", "409 - ID already in use").to_bytes());
                    return;
                }
                id = extracted_id;
            }
        }

        self.logger.info(&format!("WS: Conexão [ID: {}] estabelecida em {} (modo: {:?})", id, req.path, mode));

        match WsConnection::new(stream, mode, self.ws_hub.clone(), id, config) {
            Some(mut conn) => {
                handler(&mut conn, self.ws_hub.clone());

                let stream_fd = conn.stream.as_raw_fd();
                let _ = conn.stream.set_read_timeout(Some(std::time::Duration::from_millis(500)));

                let conn_arc = Arc::new(Mutex::new(conn));
                
                let conn_clone = Arc::clone(&conn_arc);
                let pool_clone = self.thread_pool.as_ref().unwrap().clone();
                let reactor_clone = Arc::clone(&self.reactor);
                
                let _ = self.reactor.register(stream_fd, EPOLLIN | EPOLLONESHOT, move |_| {
                    let c = Arc::clone(&conn_clone);
                    let r = Arc::clone(&reactor_clone);
                    
                    let _ = pool_clone.execute(move || {
                        let keep_alive = {
                            let mut c_guard = c.lock().unwrap();
                            c_guard.process_next_frame()
                        };
                        
                        if keep_alive {
                            let _ = r.modify(stream_fd, EPOLLIN | EPOLLONESHOT);
                        } else {
                            let _ = r.unregister(stream_fd);
                        }
                    });
                });
            },
            None => {
                self.logger.warn("WS: Conexão abortada (falha ao criar a sessão, provável esgotamento de FD)");
            }
        }
    }

    /// Resolve a requisição: tenta grupos (com middleware) primeiro, depois rotas avulsas
    fn resolve_request(&self, req: &mut HttpRequest) -> HttpResponse {
        for group in &self.groups {
            if let Some(routes_for_method) = group.routes.get(&req.method) {
                for route in routes_for_method {
                    if let Some(params) = route.matches(&req.method, &req.path) {
                        req.params = params;
                        if let Some(ref middleware) = group.middleware {
                            if let Some(blocked_response) = middleware(req) {
                                return blocked_response;
                            }
                        }
                        return (route.handler)(req.take());
                    }
                }
            }
        }

        if let Some(routes_for_method) = self.routes.get(&req.method) {
            for route in routes_for_method {
                if let Some(params) = route.matches(&req.method, &req.path) {
                    req.params = params;
                    return (route.handler)(req.take());
                }
            }
        }

        if req.method == HttpMethod::GET {
            for (prefix, dir) in &self.static_dirs {
                if req.path.starts_with(prefix) {
                    let sub_path = &req.path[prefix.len()..];
                    // Bloqueia directory traversal trivial
                    if sub_path.contains("../") || sub_path.contains("..\\") {
                        return HttpResponse::not_found("404 - Not Found");
                    }
                    
                    let mut file_path = std::path::PathBuf::from(dir);
                    file_path.push(sub_path);

                    if file_path.is_file() {
                        return Self::serve_static_file(&file_path);
                    }
                }
            }
        }

        (self.not_found_handler)(req.take())
    }

    fn serve_static_file(path: &std::path::Path) -> HttpResponse {
        use std::fs;
        match fs::read(path) {
            Ok(bytes) => {
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                let mime = match ext {
                    "html" | "htm" => "text/html",
                    "css" => "text/css",
                    "js" => "application/javascript",
                    "json" => "application/json",
                    "png" => "image/png",
                    "jpg" | "jpeg" => "image/jpeg",
                    "svg" => "image/svg+xml",
                    "wasm" => "application/wasm",
                    "txt" => "text/plain",
                    _ => "application/octet-stream",
                };
                
                let mut res = HttpResponse::new(200, "OK", "");
                res.body = bytes;
                res.headers.push(("Content-Type".to_string(), mime.to_string()));
                res
            }
            Err(_) => HttpResponse::not_found("404 - File Not Found"),
        }
    }
}
