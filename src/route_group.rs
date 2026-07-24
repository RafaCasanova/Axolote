use super::http::{HttpMethod, HttpRequest, HttpResponse};
use super::route::Route;

/// Assinatura do middleware.
/// Recebe a requisição por referência e retorna:
///   - None       → requisição liberada, continua para o handler
///   - Some(resp) → requisição bloqueada, retorna essa resposta imediatamente
pub type MiddlewareFn = Box<dyn Fn(&HttpRequest) -> Option<HttpResponse> + Send + Sync>;

/// Grupo de rotas com um prefixo comum e um middleware opcional.
/// Exemplo: grupo "/cliente" com middleware de autenticação
///   - rota "/{id}" vira "/cliente/{id}"
///   - rota "/"     vira "/cliente/"
pub struct RouteGroup {
    pub prefix: String,
    pub middleware: Option<MiddlewareFn>,
    pub routes: std::collections::HashMap<HttpMethod, Vec<Route>>,
}

impl RouteGroup {
    /// Cria um novo grupo de rotas sem middleware.
    pub fn new(prefix: &str) -> Self {
        RouteGroup {
            prefix: prefix.to_string(),
            middleware: None,
            routes: std::collections::HashMap::new(),
        }
    }

    /// Define um middleware para o grupo. Opcional — se não chamar, o grupo funciona sem.
    pub fn set_middleware<F>(&mut self, middleware: F)
    where
        F: Fn(&HttpRequest) -> Option<HttpResponse> + Send + Sync + 'static,
    {
        self.middleware = Some(Box::new(middleware));
    }

    /// Adiciona uma rota ao grupo. O path informado é relativo ao prefixo do grupo.
    /// Exemplo: grupo "/cliente" + path "/{id}" = rota final "/cliente/{id}"
    pub fn add_route<F>(&mut self, method: HttpMethod, path: &str, handler: F)
    where
        F: Fn(HttpRequest) -> HttpResponse + Send + Sync + 'static,
    {
        let full_path = format!("{}{}", self.prefix, path);
        self.routes.entry(method.clone()).or_insert_with(Vec::new).push(Route::new(method, &full_path, handler));
    }
}
