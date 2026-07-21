use crate::http::request::HttpRequest;

/// Módulo de Segurança Opcional para o Handshake WebSocket
pub struct WsSecurityGuard {
    /// Whitelist de origens permitidas (verifica o header `Origin`).
    pub allowed_origins: Option<Vec<String>>,
    /// Whitelist de subprotocolos (verifica o header `Sec-WebSocket-Protocol`).
    pub allowed_subprotocols: Option<Vec<String>>,
    /// Força validações estritas da RFC 6455 (HTTP GET, versão 13).
    pub strict_rfc: bool,
    /// Closure de validação customizada (para tokens JWT, checagem de IP, Cookies, etc).
    pub custom_validator: Option<Box<dyn Fn(&HttpRequest) -> bool + Send + Sync>>,
    /// Autenticação via Query Parameter (ex: ?api_key=XYZ)
    pub required_query_tokens: Vec<(String, String)>,
    /// Autenticação via Header (ex: Authorization: Bearer XYZ)
    pub required_header_tokens: Vec<(String, String)>,
}

impl Default for WsSecurityGuard {
    fn default() -> Self {
        WsSecurityGuard {
            allowed_origins: None,
            allowed_subprotocols: None,
            strict_rfc: true, // Por padrão, exige conformidade com a RFC
            custom_validator: None,
            required_query_tokens: Vec::new(),
            required_header_tokens: Vec::new(),
        }
    }
}

impl WsSecurityGuard {
    pub fn new() -> Self {
        Self::default()
    }

    /// Permite uma lista específica de Origins (Prevenção CSWSH).
    pub fn with_origins(mut self, origins: Vec<&str>) -> Self {
        self.allowed_origins = Some(origins.into_iter().map(|s| s.to_string()).collect());
        self
    }

    /// Permite uma lista específica de Subprotocolos (ex: "chat", "superchat").
    pub fn with_subprotocols(mut self, subprotocols: Vec<&str>) -> Self {
        self.allowed_subprotocols = Some(subprotocols.into_iter().map(|s| s.to_string()).collect());
        self
    }

    /// Desabilita a verificação estrita da RFC 6455 (Não recomendado).
    pub fn allow_non_strict(mut self) -> Self {
        self.strict_rfc = false;
        self
    }

    /// Injeta um validador customizado (ex: extrair e validar tokens do path ou headers).
    pub fn with_validator<F>(mut self, validator: F) -> Self
    where
        F: Fn(&HttpRequest) -> bool + Send + Sync + 'static,
    {
        self.custom_validator = Some(Box::new(validator));
        self
    }

    /// Exige que a conexão possua um Query Parameter com um valor específico (ex: "api_key", "SECRET_123")
    pub fn with_query_token(mut self, param: &str, expected_value: &str) -> Self {
        self.required_query_tokens.push((param.to_string(), expected_value.to_string()));
        self
    }

    /// Exige que a conexão possua um Header HTTP com um valor específico (ex: "Authorization", "Bearer SECRET_123")
    pub fn with_header_token(mut self, header: &str, expected_value: &str) -> Self {
        self.required_header_tokens.push((header.to_string(), expected_value.to_string()));
        self
    }
}
