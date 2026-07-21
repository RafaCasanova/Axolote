/// Representa os métodos HTTP suportados
#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub enum HttpMethod {
    GET,
    POST,
    PUT,
    DELETE,
    PATCH,
    CONNECT,
    UNKNOWN(String),
}

impl HttpMethod {
    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "GET" => HttpMethod::GET,
            "POST" => HttpMethod::POST,
            "PUT" => HttpMethod::PUT,
            "DELETE" => HttpMethod::DELETE,
            "PATCH" => HttpMethod::PATCH,
            "CONNECT" => HttpMethod::CONNECT,
            other => HttpMethod::UNKNOWN(other.to_string()),
        }
    }
}

