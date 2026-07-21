extern crate axolote;
use axolote::prelude::*;

/// Handler simples que retorna um texto OK
fn handler_home(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Bem vindo ao Axolote!")
}

/// Handler simples que retorna informações de saúde da API
fn handler_health(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("API está saudavel. Status: OK")
}

fn main() {
    // Cria o servidor na porta 8081
    let mut server = Server::new("8081");

    // Adiciona rotas avulsas (GET)
    server.add_route(HttpMethod::GET, "/", handler_home);
    server.add_route(HttpMethod::GET, "/health", handler_health);

    // Roda o servidor
    server.run();
}
