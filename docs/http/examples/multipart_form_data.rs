extern crate axolote;

use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn process_form(req: HttpRequest) -> HttpResponse {
    let form = req.form_data();
    
    // Mostra o que chegou processado no console
    println!("Formulário recebido:");
    for (k, v) in &form {
        println!("  {} = {}", k, v);
    }

    if let Some(nome) = form.get("nome") {
        let msg = format!("Olá, {}! Seus dados foram processados com sucesso.", nome);
        HttpResponse::ok(msg)
    } else {
        HttpResponse::bad_request("Campo 'nome' é obrigatório!")
    }
}

fn main() {
    let mut server = Server::new("8080");

    // Endpoint para receber o formulário
    server.add_route(HttpMethod::POST, "/submit", process_form);

    println!("Servidor rodando em http://127.0.0.1:8080");
    println!("Para testar, envie um POST simulando um formulário HTML:");
    println!("curl -X POST http://127.0.0.1:8080/submit -d 'nome=Rafael%20Casanova&profissao=Engenheiro+de+Software'");
    
    server.run();
}
