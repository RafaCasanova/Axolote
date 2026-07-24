use std::collections::HashMap;
use super::http::{HttpMethod, HttpRequest, HttpResponse};

/// Assinatura para as funções handler baseadas em Closures
pub type HandlerFn = Box<dyn Fn(HttpRequest) -> HttpResponse + Send + Sync>;

/// Estrutura que mapeia um método HTTP, um caminho (path) e uma função handler
pub struct Route {
    pub method: HttpMethod,
    pub path: String,
    pub handler: HandlerFn,
}

impl Route {
    pub fn new<F>(method: HttpMethod, path: &str, handler: F) -> Self
    where
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        Route {
            method,
            path: path.to_string(),
            handler: Box::new(handler),
        }
    }

    /// Verifica se a rota condiz com a requisição e extrai os parâmetros (ex: {id})
    /// Retorna Some(params) se a rota combinou, None caso contrário.
    pub fn matches(&self, req_method: &HttpMethod, req_path: &str) -> Option<HashMap<String, String>> {
        if &self.method != req_method {
            return None;
        }

        let route_parts: Vec<&str> = self.path.split('/').filter(|s| !s.is_empty()).collect();
        let req_parts: Vec<&str> = req_path.split('/').filter(|s| !s.is_empty()).collect();

        if route_parts.len() != req_parts.len() {
            return None;
        }

        let mut params = HashMap::new();

        for (r_part, req_part) in route_parts.iter().zip(req_parts.iter()) {
            if let (Some(start), Some(end)) = (r_part.find('{'), r_part.find('}')) {
                let prefix = &r_part[..start];
                let suffix = &r_part[end + 1..];
                let raw_key = &r_part[start + 1..end];
                let (key, param_type) = match raw_key.find(':') {
                    Some(idx) => (&raw_key[..idx], Some(&raw_key[idx + 1..])),
                    None => (raw_key, None),
                };

                if req_part.starts_with(prefix) && req_part.ends_with(suffix) && req_part.len() >= prefix.len() + suffix.len() {
                    let value_len = req_part.len() - prefix.len() - suffix.len();
                    let value = &req_part[prefix.len()..prefix.len() + value_len];
                    
                    // Validador tipado (micro-regex)
                    let is_valid = match param_type {
                        Some("num") => !value.is_empty() && value.chars().all(|c| c.is_ascii_digit()),
                        Some("alpha") => !value.is_empty() && value.chars().all(|c| c.is_ascii_alphabetic()),
                        Some("alnum") => !value.is_empty() && value.chars().all(|c| c.is_ascii_alphanumeric()),
                        _ => true,
                    };

                    if !is_valid {
                        return None;
                    }

                    params.insert(key.to_string(), value.to_string());
                } else {
                    return None;
                }
            } else if r_part != req_part {
                return None;
            }
        }

        Some(params)
    }
}
