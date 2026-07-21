pub mod http;
pub mod logger;
pub mod route;
pub mod route_group;
pub mod thread_pool;
pub mod ws;

/// Módulo Prelude para facilitar a importação em projetos externos
pub mod prelude {
    pub use crate::Server;
    pub use crate::http::{HttpMethod, HttpRequest, HttpResponse};
    pub use crate::route_group::RouteGroup;
    pub use crate::logger::{Logger, LoggerConfig, LogFormat, LogTarget, LogLevel, LogDispatcher};
    pub use crate::ws::{WsConnection, WsMode, WsMessage, WsHandlerFn, WsHub, WsRouteConfig};
    pub use crate::ws::cluster::ClusterConfig; // NEW
}

use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;

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
    not_found_handler: HandlerFn,
    pub logger: crate::logger::Logger,
    pub timeout: std::time::Duration,
}

impl Server {
    /// Instancia o servidor com a porta informada (IPv4 por padrão)
    pub fn new(port: &str) -> Self {
        Server {
            address: format!("127.0.0.1:{}", port),
            routes: std::collections::HashMap::new(),
            groups: Vec::new(),
            ws_routes: Vec::new(),
            ws_hub: WsHub::new(),
            cluster_config: None, // NEW
            not_found_handler: default_not_found_handler,
            logger: crate::logger::Logger::new(crate::logger::LoggerConfig::new(vec![
                    crate::logger::LogDispatcher {
                        min_level: crate::logger::LogLevel::Info,
                        format: crate::logger::LogFormat::Text,
                        target: crate::logger::LogTarget::Console,
                    }
                ]
            )),
            timeout: std::time::Duration::from_secs(10),
        }
    }

    /// Instancia o servidor utilizando IPv6
    pub fn new_ipv6(port: &str) -> Self {
        Server {
            address: format!("[::1]:{}", port),
            routes: std::collections::HashMap::new(),
            groups: Vec::new(),
            ws_routes: Vec::new(),
            ws_hub: WsHub::new(),
            cluster_config: None, // NEW
            not_found_handler: default_not_found_handler,
            logger: crate::logger::Logger::new(crate::logger::LoggerConfig::new(vec![
                    crate::logger::LogDispatcher {
                        min_level: crate::logger::LogLevel::Info,
                        format: crate::logger::LogFormat::Text,
                        target: crate::logger::LogTarget::Console,
                    }
                ]
            )),
            timeout: std::time::Duration::from_secs(10),
        }
    }

    /// Habilita o modo cluster distribuido
    pub fn enable_cluster(&mut self, config: ClusterConfig) {
        self.cluster_config = Some(config);
    }

    /// Define o timeout de leitura e escrita para as conexões TCP em segundos (padrão: 10s)
    pub fn set_timeout(&mut self, seconds: u64) {
        self.timeout = std::time::Duration::from_secs(seconds);
    }

    /// Define um handler customizado para rotas 404 (Not Found)
    pub fn set_not_found_handler(&mut self, handler: HandlerFn) {
        self.not_found_handler = handler;
    }

    /// Adiciona uma rota avulsa ao servidor (sem grupo/middleware)
    pub fn add_route(&mut self, method: HttpMethod, path: &str, handler: HandlerFn) {
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
            let state = ClusterState::new(cfg.node_id);
            self.ws_hub.enable_cluster(state.clone());
            ClusterManager::start(cfg, state, self.ws_hub.clone());
        } else {
            self.logger.info("Modo Cluster desativado (Standalone)");
        }

        self.logger.info(&format!("Servidor rodando em http://{}", self.address));

        let server = Arc::new(self);
        
        // Determina o número de threads com base no paralelismo disponível (CPUs lógicas)
        let num_threads = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4) * 4;
            
        server.logger.info(&format!("Inicializando ThreadPool com {} threads.", num_threads));
        let pool = crate::thread_pool::ThreadPool::new(num_threads);

        for stream in listener.incoming() {
            match stream {
                Ok(stream) => {
                    let srv = Arc::clone(&server);
                    pool.execute(move || {
                        srv.handle_connection(stream);
                    });
                }
                Err(e) => {
                    server.logger.error(&format!("Erro ao aceitar conexão: {}", e));
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
        let mut headers_ended = false;
        let mut content_length = 0;
        let mut headers_len = 0;
        const MAX_BODY_SIZE: usize = 10 * 1024 * 1024; // 10 MB

        loop {
            match stream.read(&mut buffer) {
                Ok(0) => break, // EOF
                Ok(n) => {
                    raw_data.extend_from_slice(&buffer[..n]);

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
                        }
                    }

                    if headers_ended {
                        let body_read = raw_data.len() - headers_len;
                        if body_read >= content_length {
                            break;
                        }
                    }
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
            self.logger.warn("Conexão recebida sem dados ou encerrada precocemente");
            return;
        }

        let request_str = String::from_utf8_lossy(&raw_data);

        if let Some(mut req) = HttpRequest::from_raw(&request_str) {
            // Verifica se é um pedido de Upgrade para WebSocket
            if ws::handshake::is_websocket_upgrade(&req.headers) {
                self.handle_websocket(stream, &req);
                return;
            }

            let response = self.resolve_request(&mut req);
            self.logger.info(&format!(
                "{:?} {} - {} {}",
                req.method, req.path, response.status_code, response.status_text
            ));
            let _ = stream.write_all(&response.to_bytes());
        } else {
            let response = HttpResponse::bad_request("400 - Requisicao invalida");
            self.logger.warn("Requisição inválida recebida");
            let _ = stream.write_all(&response.to_bytes());
        }
        let _ = stream.flush();
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
                if actual != Some(expected) {
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
                if actual != Some(expected) {
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
        
        let id = ws::hub::next_connection_id();
        self.logger.info(&format!("WS: Conexão [ID: {}] estabelecida em {} (modo: {:?})", id, req.path, mode));

        match WsConnection::new(stream, mode, self.ws_hub.clone(), id, config) {
            Some(conn) => handler(conn, self.ws_hub.clone()),
            None => {
                self.logger.warn("WS: Conexão abortada (falha ao criar a sessão, provável esgotamento de FD)");
                return;
            }
        }

        self.logger.info(&format!("WS: Conexão [ID: {}] encerrada.", id));
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

        (self.not_found_handler)(req.take())
    }
}
