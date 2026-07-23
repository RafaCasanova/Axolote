extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn search_handler(req: HttpRequest) -> HttpResponse {
    let termo = req.query_params.get("q").cloned().unwrap_or_else(|| "Nenhum".to_string());
    let limite = req.query_params.get("limite").cloned().unwrap_or_else(|| "10".to_string());

    HttpResponse::ok(format!(
        "Resultado da busca pelo termo: '{}' (Limite: {})", 
        termo, limite
    ))
}

fn main() {
    let mut server = Server::new("8080");

    // Rota que processa Query String: /busca?q=rust&limite=50
    server.add_route(HttpMethod::GET, "/busca", search_handler);

    server.run();
}
