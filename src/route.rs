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
}
