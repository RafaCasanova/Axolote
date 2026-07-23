extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn home_handler(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Bem-vindo a Pagina Inicial")
}

fn info_handler(req: HttpRequest) -> HttpResponse {
    let client_id = req.params.get("id").cloned().unwrap_or_else(|| "Nenhum".to_string());
    HttpResponse::ok(format!("Informacoes do Usuario ID: {}", client_id))
}

fn create_user_handler(req: HttpRequest) -> HttpResponse {
    let body_content = req.body;
    HttpResponse::created(format!("Usuario criado com sucesso. Dados: {}", body_content))
}

fn main() {
    let mut server = Server::new("8080");

    // Rotas simples sem middlewares
    server.add_route(HttpMethod::GET, "/", home_handler);
    
    // Rota com parametro dinamico (path parameter)
    server.add_route(HttpMethod::GET, "/usuario/:id", info_handler);
    
    // Rota POST com leitura de corpo de requisicao
    server.add_route(HttpMethod::POST, "/usuario", create_user_handler);

    server.run();
}
