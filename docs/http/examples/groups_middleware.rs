extern crate axolote;
use axolote::prelude::*;

// Middleware de autenticacao simples baseado em headers
fn auth_middleware(req: &HttpRequest) -> Option<HttpResponse> {
    if let Some(token) = req.headers.get("Authorization") {
        if token == "Bearer token_secreto" {
            return None; // Autenticado, prossegue para o handler
        }
    }
    // Retorna 401 caso o token nao esteja correto
    Some(HttpResponse::new(401, "Unauthorized", "Acesso Negado: Token Invalido"))
}

fn dashboard_handler(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Acesso concedido a Area Administrativa")
}

fn profile_handler(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Informacoes protegidas do usuario autenticado")
}

fn main() {
    let mut server = Server::new("8080");

    // Cria um grupo sob o prefixo "/admin"
    let mut admin_group = RouteGroup::new("/admin");
    
    // Associa o middleware de autenticacao
    admin_group.set_middleware(auth_middleware);

    // Adiciona rotas internas ao grupo
    admin_group.add_route(HttpMethod::GET, "/dashboard", dashboard_handler);
    admin_group.add_route(HttpMethod::GET, "/perfil", profile_handler);

    // Registra o grupo no servidor
    server.add_group(admin_group);

    server.run();
}
