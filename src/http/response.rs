use std::collections::HashMap;

/// Estrutura que encapsula a resposta HTTP (struct Write)
/// Todo handler obrigatoriamente retorna essa struct.
pub struct HttpResponse {
    pub status_code: u16,
    pub status_text: String,
    pub headers: HashMap<String, String>,
    pub body: String,
}

impl HttpResponse {
    pub fn new(status_code: u16, status_text: &str, body: &str) -> Self {
        let mut headers = HashMap::new();
        headers.insert("Content-Type".to_string(), "text/plain; charset=utf-8".to_string());
        headers.insert("Content-Length".to_string(), body.len().to_string());

        HttpResponse {
            status_code,
            status_text: status_text.to_string(),
            headers,
            body: body.to_string(),
        }
    }

    pub fn ok(body: &str) -> Self {
        Self::new(200, "OK", body)
    }

    pub fn created(body: &str) -> Self {
        Self::new(201, "Created", body)
    }

    pub fn not_found(body: &str) -> Self {
        Self::new(404, "Not Found", body)
    }

    pub fn bad_request(body: &str) -> Self {
        Self::new(400, "Bad Request", body)
    }

    pub fn internal_error(body: &str) -> Self {
        Self::new(500, "Internal Server Error", body)
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
