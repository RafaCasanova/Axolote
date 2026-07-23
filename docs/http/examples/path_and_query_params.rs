extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

/// Exemplo: Extração de Path Parameters (/usuario/123)
fn handler_ver_usuario(req: HttpRequest) -> HttpResponse {
    // req.params guarda os valores dinâmicos da URL definidos por chaves {}
    let id = req.params.get("id").cloned().unwrap_or_default();
    
    HttpResponse::ok(format!("Mostrando perfil do usuario ID: {}", id))
}

/// Exemplo: Extração de Query Parameters (/busca?termo=Rust&page=2)
fn handler_buscar(req: HttpRequest) -> HttpResponse {
    // req.query_params guarda as chaves e valores passados após o ? na URL
    let termo = req.query_params.get("termo").cloned().unwrap_or_else(|| "Nenhum termo".to_string());
    let page = req.query_params.get("page").cloned().unwrap_or_else(|| "1".to_string());
    
    let resposta = format!("Buscando por: '{}' na página {}", termo, page);
    HttpResponse::ok(resposta)
}

fn main() {
    let mut server = Server::new("8082");

    // Rota com Path Parameter tipado (apenas números)
    server.add_route(HttpMethod::GET, "/usuario/{id:num}", handler_ver_usuario);
    
    // Rota com Query Parameter
    server.add_route(HttpMethod::GET, "/busca", handler_buscar);

    server.run();
}
