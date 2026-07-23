extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};
use axolote::route_group::RouteGroup;

/// Middleware de Autenticação Falsa
/// Retorna `None` se a requisição puder passar para o handler.
/// Retorna `Some(HttpResponse)` se quiser bloquear a requisição.
fn middleware_auth(req: &HttpRequest) -> Option<HttpResponse> {
    if req.headers.contains_key("Authorization") {
        println!("[Auth Middleware] Passagem autorizada.");
        None
    } else {
        println!("[Auth Middleware] Bloqueado. Header Authorization ausente.");
        Some(HttpResponse::new(401, "Unauthorized", "401 - Acesso negado. Envie o header Authorization."))
    }
}

/// Handler protegido
fn handler_dashboard(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Bem-vindo ao Dashboard Confidencial!")
}

fn main() {
    let mut server = Server::new("8083");

    // Cria um grupo de rotas sob o prefixo "/admin"
    let mut admin_group = RouteGroup::new("/admin");
    
    // Adiciona o middleware de proteção a este grupo
    admin_group.set_middleware(middleware_auth);
    
    // Adiciona rotas ao grupo (a rota real será /admin/dashboard)
    admin_group.add_route(HttpMethod::GET, "/dashboard", handler_dashboard);

    // Registra o grupo no servidor
    server.add_group(admin_group);

    server.run();
}
