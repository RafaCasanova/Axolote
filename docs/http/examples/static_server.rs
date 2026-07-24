extern crate axolote;

use axolote::Server;

fn main() {
    let mut server = Server::new("8080");

    // Configurando servidor de arquivos estáticos
    // Qualquer acesso em "/static/..." buscará na pasta "./public"
    // Isso é super útil para rodar seu frontend web Vue, React ou Angular!
    server.serve_dir("/static", "./public");

    println!("Servidor estático rodando em http://127.0.0.1:8080");
    println!("Crie a pasta 'public' e um arquivo 'index.html' nela.");
    println!("Acesse http://127.0.0.1:8080/static/index.html");
    
    server.run();
}
