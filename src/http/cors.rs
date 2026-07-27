use super::request::HttpRequest;
use super::response::HttpResponse;

/// Configuração do Middleware de CORS (Cross-Origin Resource Sharing)
#[derive(Clone)]
pub struct CorsConfig {
    pub allow_origins: Vec<String>,
    pub allow_methods: Vec<String>,
    pub allow_headers: Vec<String>,
    pub expose_headers: Vec<String>,
    pub max_age: Option<usize>,
    pub allow_credentials: bool,
}

impl Default for CorsConfig {
    /// Cria uma configuracao relaxada (permite tudo) para agilizar o desenvolvimento.
    /// NOTA: allow_credentials e false por padrao porque a spec CORS proibe
    /// combinar Access-Control-Allow-Origin: * com Access-Control-Allow-Credentials: true.
    /// Para usar credenciais, chame .with_credentials(true) junto com origens explicitas.
    fn default() -> Self {
        CorsConfig {
            allow_origins: vec!["*".to_string()],
            allow_methods: vec![
                "GET".to_string(),
                "POST".to_string(),
                "PUT".to_string(),
                "DELETE".to_string(),
                "OPTIONS".to_string(),
                "PATCH".to_string(),
            ],
            allow_headers: vec!["*".to_string()],
            expose_headers: vec![],
            max_age: Some(86400),
            allow_credentials: false,
        }
    }
}

impl CorsConfig {
    /// Inicializa a configuração padrão.
    pub fn new() -> Self {
        Self::default()
    }

    /// Cria uma configuração restritiva permitindo apenas origens específicas.
    pub fn restrictive(origins: Vec<&str>) -> Self {
        let mut config = Self::default();
        config.allow_origins = origins.into_iter().map(|s| s.to_string()).collect();
        config
    }

    pub fn with_methods(mut self, methods: Vec<&str>) -> Self {
        self.allow_methods = methods.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_headers(mut self, headers: Vec<&str>) -> Self {
        self.allow_headers = headers.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_expose_headers(mut self, headers: Vec<&str>) -> Self {
        self.expose_headers = headers.into_iter().map(|s| s.to_string()).collect();
        self
    }

    pub fn with_max_age(mut self, max_age: usize) -> Self {
        self.max_age = Some(max_age);
        self
    }

    pub fn with_credentials(mut self, allow: bool) -> Self {
        self.allow_credentials = allow;
        self
    }

    /// Injeta os cabecalhos de CORS em uma resposta final (GET, POST, etc)
    pub fn apply_to_response(&self, req: &HttpRequest, mut res: HttpResponse) -> HttpResponse {
        let req_origin = req.headers.get("origin").map(|s| s.as_str()).unwrap_or("*");
        let is_wildcard = self.allow_origins.contains(&"*".to_string());

        if is_wildcard {
            res.headers.push(("Access-Control-Allow-Origin".to_string(), "*".to_string()));
        } else if self.allow_origins.iter().any(|o| o == req_origin) {
            res.headers.push(("Access-Control-Allow-Origin".to_string(), req_origin.to_string()));
            res.headers.push(("Vary".to_string(), "Origin".to_string()));
        }

        // RFC: Allow-Credentials: true so e valido com origem explicita, nunca com *
        if self.allow_credentials && !is_wildcard {
            res.headers.push(("Access-Control-Allow-Credentials".to_string(), "true".to_string()));
        }

        if !self.expose_headers.is_empty() {
            res.headers.push(("Access-Control-Expose-Headers".to_string(), self.expose_headers.join(", ")));
        }

        res
    }

    /// Lida com a requisicao de pre-voo (OPTIONS) enviada pelos navegadores.
    pub fn handle_preflight(&self, req: &HttpRequest) -> HttpResponse {
        let mut res = HttpResponse::new(204, "No Content", "");

        let req_origin = req.headers.get("origin").map(|s| s.as_str()).unwrap_or("*");
        let is_wildcard = self.allow_origins.contains(&"*".to_string());

        // Tratamento da Origem
        if is_wildcard {
            res.headers.push(("Access-Control-Allow-Origin".to_string(), "*".to_string()));
        } else if self.allow_origins.iter().any(|o| o == req_origin) {
            res.headers.push(("Access-Control-Allow-Origin".to_string(), req_origin.to_string()));
            res.headers.push(("Vary".to_string(), "Origin".to_string()));
        }

        // Metodos permitidos
        res.headers.push(("Access-Control-Allow-Methods".to_string(), self.allow_methods.join(", ")));

        // Cabecalhos permitidos
        if self.allow_headers.contains(&"*".to_string()) {
            if let Some(req_headers) = req.headers.get("access-control-request-headers") {
                res.headers.push(("Access-Control-Allow-Headers".to_string(), req_headers.to_string()));
            } else {
                res.headers.push(("Access-Control-Allow-Headers".to_string(), "*".to_string()));
            }
        } else {
            res.headers.push(("Access-Control-Allow-Headers".to_string(), self.allow_headers.join(", ")));
        }

        // Cache de preflight
        if let Some(max_age) = self.max_age {
            res.headers.push(("Access-Control-Max-Age".to_string(), max_age.to_string()));
        }

        // RFC: Allow-Credentials: true so e valido com origem explicita, nunca com *
        if self.allow_credentials && !is_wildcard {
            res.headers.push(("Access-Control-Allow-Credentials".to_string(), "true".to_string()));
        }

        res
    }
}
