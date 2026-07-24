extern crate axolote;

use axolote::prelude::*;
use axolote::http::cors::CorsConfig;

fn public_api(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Acesso liberado de qualquer origem! (Graças ao CORS)")
}

fn private_api(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Dados restritos. (Acessado de origens permitidas)")
}

fn main() {
    let mut server = Server::new("8080");

    // 1. Configurando CORS Nativo!
    // Você pode usar `CorsConfig::default()` que libera tudo,
    // ou personalizar a configuração restrita, como no exemplo abaixo:
    let cors = CorsConfig::default();
    
    // Suponha que queremos restringir origens em produção:
    // cors.allow_origins = vec!["http://localhost:3000".to_string(), "https://meu-app.com".to_string()];
    // cors.allow_methods = vec!["GET".to_string(), "POST".to_string(), "OPTIONS".to_string()];

    // Ligamos o middleware nativo de CORS no servidor.
    // Isso fará com que o servidor passe a responder requisições OPTIONS automaticamente 
    // com 204 No Content, e insira os headers Access-Control-* nas respostas de rotas.
    server.enable_cors(cors);

    server.add_route(HttpMethod::GET, "/public", public_api);
    server.add_route(HttpMethod::GET, "/private", private_api);

    println!("Servidor com CORS ativado! Inicie seu Frontend React/Vue em http://localhost:3000");
    println!("Faça requisições para http://localhost:8080/public");
    server.run();
}
