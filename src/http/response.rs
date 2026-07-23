use std::collections::HashMap;

pub trait IntoBody {
    fn into_body(self) -> (String, String);
}

impl IntoBody for &str {
    fn into_body(self) -> (String, String) {
        (self.to_string(), "text/plain; charset=utf-8".to_string())
    }
}

impl IntoBody for String {
    fn into_body(self) -> (String, String) {
        (self, "text/plain; charset=utf-8".to_string())
    }
}

// A implementação para structs customizadas é gerada automaticamente pelo #[axolote_json]

/// Estrutura que encapsula a resposta HTTP (struct Write)
/// Todo handler obrigatoriamente retorna essa struct.
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl HttpResponse {
    pub fn new<B: IntoBody>(status_code: u16, status_text: &str, body: B) -> Self {
        let (body_str, content_type) = body.into_body();
        
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), content_type);
        headers.insert("Content-Length".to_string(), body_str.len().to_string());
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

    /// Adiciona um header customizado (Builder Pattern)
    pub fn with_header(mut self, key: &str, value: &str) -> Self {
        self.headers.insert(key.to_string(), value.to_string());
        self
    }

    /// Adiciona um cookie (Builder Pattern)
    pub fn with_cookie(self, key: &str, value: &str) -> Self {
        self.with_header("Set-Cookie", &format!("{}={}", key, value))
    }

    /// Serializa a struct em bytes seguindo o protocolo HTTP/1.1
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut response = format!("HTTP/1.1 {} {}\r\n", self.status_code, self.status_text);
        for (key, value) in &self.headers {
            response.push_str(&format!("{}: {}\r\n", key, value));
        }
        response.push_str("\r\n");
        response.push_str(&self.body);
        response.into_bytes()
    }
}
