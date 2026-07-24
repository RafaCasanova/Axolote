use std::collections::HashMap;
use super::method::HttpMethod;

/// Estrutura que encapsula a requisição HTTP recebida
pub struct HttpRequest {
    pub method: HttpMethod,
    pub path: String,
    pub params: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

impl HttpRequest {
    /// Faz o parse dos bytes brutos da requisição HTTP e devolve um HttpRequest
    pub fn from_bytes(raw: &[u8]) -> Option<Self> {
        let mut headers_end = 0;
        for i in 0..raw.len().saturating_sub(3) {
            if &raw[i..i+4] == b"\r\n\r\n" {
                headers_end = i + 4;
                break;
            }
        }
        if headers_end == 0 {
            for i in 0..raw.len().saturating_sub(1) {
                if &raw[i..i+2] == b"\n\n" {
                    headers_end = i + 2;
                    break;
                }
            }
        }
        if headers_end == 0 {
            return None;
        }

        let header_str = String::from_utf8_lossy(&raw[..headers_end]);
        let mut lines = header_str.lines();
        let request_line = lines.next()?;
        let parts: Vec<&str> = request_line.split_whitespace().collect();
        if parts.len() < 3 {
            return None;
        }

        let method = HttpMethod::from_str(parts[0]);
        let raw_path = parts[1];

        // Separar path e query params
        let (path, query_params) = if let Some(pos) = raw_path.find('?') {
            let p = raw_path[..pos].to_string();
            let query_string = &raw_path[pos + 1..];
            let mut qp = HashMap::new();
            for pair in query_string.split('&') {
                if let Some((key, value)) = pair.split_once('=') {
                    qp.insert(Self::url_decode(key), Self::url_decode(value));
                } else if !pair.is_empty() {
                    qp.insert(Self::url_decode(pair), String::new());
                }
            }
            (p, qp)
        } else {
            (raw_path.to_string(), HashMap::new())
        };

        let mut headers = HashMap::new();

        for line in lines {
            if line.trim().is_empty() {
                continue;
            }

            if let Some((key, value)) = line.split_once(':') {
                headers.insert(key.trim().to_lowercase(), value.trim().to_string());
            }
        }

        let body = raw[headers_end..].to_vec();

        Some(HttpRequest {
            method,
            path,
            params: HashMap::new(),
            query_params,
            headers,
            body,
        })
    }

    /// Retorna o corpo da requisição formatado como string UTF-8 (útil para payloads JSON/Texto)
    pub fn body_utf8(&self) -> String {
        String::from_utf8_lossy(&self.body).trim_end_matches('\0').trim().to_string()
    }

    /// Extrai o conteúdo do HttpRequest a partir de um &mut self,
    /// deixando valores padrão no lugar. Permite converter &mut → owned.
    pub fn take(&mut self) -> Self {
        use std::mem;
        HttpRequest {
            method: mem::replace(&mut self.method, HttpMethod::GET),
            path: mem::take(&mut self.path),
            params: mem::take(&mut self.params),
            query_params: mem::take(&mut self.query_params),
            headers: mem::take(&mut self.headers),
            body: mem::take(&mut self.body),
        }
    }

    /// Remove o corpo da requisição
    pub fn take_body(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.body)
    }



    /// Processa o corpo da requisição como Form Data (application/x-www-form-urlencoded)
    pub fn form_data(&self) -> HashMap<String, String> {
        let mut form = HashMap::new();
        let body_str = self.body_utf8();
        for pair in body_str.split('&') {
            if let Some((k, v)) = pair.split_once('=') {
                form.insert(Self::url_decode(k), Self::url_decode(v));
            } else if !pair.is_empty() {
                form.insert(Self::url_decode(pair), String::new());
            }
        }
        form
    }

    /// Decodifica strings URL Encoded (ex: %20 -> espaço, + -> espaço)
    pub fn url_decode(input: &str) -> String {
        let mut out = Vec::new();
        let bytes = input.as_bytes();
        let mut i = 0;
        while i < bytes.len() {
            if bytes[i] == b'+' {
                out.push(b' ');
                i += 1;
            } else if bytes[i] == b'%' && i + 2 < bytes.len() {
                let hex = std::str::from_utf8(&bytes[i + 1..=i + 2]).unwrap_or("");
                if let Ok(b) = u8::from_str_radix(hex, 16) {
                    out.push(b);
                } else {
                    out.push(b'%');
                    out.push(bytes[i + 1]);
                    out.push(bytes[i + 2]);
                }
                i += 3;
            } else {
                out.push(bytes[i]);
                i += 1;
            }
        }
        String::from_utf8_lossy(&out).into_owned()
    }
}
