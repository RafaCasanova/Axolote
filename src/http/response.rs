

pub trait IntoBody {
    fn into_body(self) -> (Vec<u8>, String);
}

impl IntoBody for &str {
    fn into_body(self) -> (Vec<u8>, String) {
        (self.as_bytes().to_vec(), "text/plain; charset=utf-8".to_string())
    }
}

impl IntoBody for String {
    fn into_body(self) -> (Vec<u8>, String) {
        (self.into_bytes(), "text/plain; charset=utf-8".to_string())
    }
}

impl IntoBody for Vec<u8> {
    fn into_body(self) -> (Vec<u8>, String) {
        (self, "application/octet-stream".to_string())
    }
}

impl IntoBody for (Vec<u8>, String) {
    fn into_body(self) -> (Vec<u8>, String) {
        self
    }
}

// A implementação para structs customizadas é gerada automaticamente pelo #[axolote_json]

/// Estrutura que encapsula a resposta HTTP (struct Write)
/// Todo handler obrigatoriamente retorna essa struct.
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

impl HttpResponse {
    pub fn new<B: IntoBody>(status_code: u16, status_text: &str, body: B) -> Self {
        let (body_str, content_type) = body.into_body();
        
        let mut headers = Vec::new();
        headers.push(("Content-Type".to_string(), content_type));
        headers.push(("Content-Length".to_string(), body_str.len().to_string()));
        // For JSON, we previously added Connection: close. Let's keep headers simple unless needed.
        // The framework's default behavior handles it.

        HttpResponse {
            status_code,
            status_text: status_text.to_string(),
            headers,
            body: body_str,
        }
    }

    pub fn ok<B: IntoBody>(body: B) -> Self {
        Self::new(200, "OK", body)
    }

    pub fn created<B: IntoBody>(body: B) -> Self {
        Self::new(201, "Created", body)
    }

    pub fn not_found<B: IntoBody>(body: B) -> Self {
        Self::new(404, "Not Found", body)
    }

    pub fn bad_request<B: IntoBody>(body: B) -> Self {
        Self::new(400, "Bad Request", body)
    }

    pub fn internal_error<B: IntoBody>(body: B) -> Self {
        Self::new(500, "Internal Server Error", body)
    }

    pub fn internal_server_error<B: IntoBody>(body: B) -> Self {
        Self::internal_error(body)
    }

    pub fn unauthorized<B: IntoBody>(body: B) -> Self {
        Self::new(401, "Unauthorized", body)
    }

    pub fn redirect(location: &str) -> Self {
        Self::new(302, "Found", "").with_header("Location", location)
    }

    /// Adiciona um header customizado (Builder Pattern)
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        // Remover chave antiga se existir, para comportamento padrão de map
        // exceto para Set-Cookie que permite múltiplos
        if key != "Set-Cookie" {
            self.headers.retain(|(k, _)| k != key);
        }
        self.headers.push((key.to_string(), value.to_string()));
        self
    }

    /// Adiciona um cookie (Builder Pattern)
    pub fn with_cookie(self, key: &str, value: &str) -> Self {
        self.with_header("Set-Cookie", &format!("{}={}", key, value))
    }

    /// Adiciona um cookie com atributos de segurança adicionais
    pub fn with_cookie_secure(self, key: &str, value: &str, path: &str, http_only: bool, secure: bool) -> Self {
        let mut cookie = format!("{}={}; Path={}", key, value, path);
        if http_only {
            cookie.push_str("; HttpOnly");
        }
        if secure {
            cookie.push_str("; Secure");
        }
        self.with_header("Set-Cookie", &cookie)
    }

    /// Serializa a struct em bytes seguindo o protocolo HTTP/1.1
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut response = format!("HTTP/1.1 {} {}\r\n", self.status_code, self.status_text);
        for (key, value) in &self.headers {
            response.push_str(&format!("{}: {}\r\n", key, value));
        }
        response.push_str("\r\n");
        let mut bytes = response.into_bytes();
        bytes.extend_from_slice(&self.body);
        bytes
    }
}
