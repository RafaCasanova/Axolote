extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};
use axolote::logger::{Logger, LoggerConfig, LogFormat, LogTarget, LogLevel, LogDispatcher};

/// Handler customizado para interceptar erros 404 (Not Found)
fn handler_404_custom(_req: HttpRequest) -> HttpResponse {
    let html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Erro 404 - Axolote</title></head>
        <body>
            <h1>Ops, página não encontrada!</h1>
            <p>O servidor em IPv6 interceptou este erro.</p>
        </body>
        </html>
    "#;
    
    let mut res = HttpResponse::not_found(html);
    res.headers.insert("Content-Type".to_string(), "text/html; charset=utf-8".to_string());
    res
}

/// Handler simples
fn handler_home(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Servidor IPv6 rodando com sucesso!")
}

fn main() {

    // Cria o servidor utilizando a interface IPv6 ([::1])
    let mut server = Server::new_ipv6("8084");

    // Configurando o Logger com 2 Despachantes Paralelos
    server.logger = Logger::new(
        LoggerConfig::new(vec![
            // 1. Imprime TUDO no terminal
            LogDispatcher {
                min_level: LogLevel::Info,
                format: LogFormat::Text,
                target: LogTarget::Console,
            },
            // 2. Salva APENAS ERROS em arquivo JSON
            LogDispatcher {
                min_level: LogLevel::Error,
                format: LogFormat::Json,
                target: LogTarget::LocalFile { 
                    dir_path: "logs".to_string(), 
                    prefix: "erros".to_string(),
                    retention_days: 5 
                },
            }
        ])
        .set_heart_beat(500) // Timeout da Thread em 500ms
        .set_cleanup_interval(3600) // Rotação roda a cada 1 hora
    );

    server.set_not_found_handler(handler_404_custom);

    // Registra uma rota na raiz
    server.add_route(HttpMethod::GET, "/", handler_home);

    server.run();
}
