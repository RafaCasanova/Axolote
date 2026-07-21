extern crate axolote;
use axolote::prelude::*;

fn handler_simular_erros(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Erros simulados com sucesso. Verifique o console e os logs HTTP.")
}

fn main() {
    println!("Iniciando exemplo 05 - Envios de Log HTTP...");

    let mut server = Server::new("8085");

    // Configurando o Logger para disparar mensagens na rede
    server.logger = Logger::new(LoggerConfig::new(vec![
            // 1. Terminal (Para podermos enxergar tudo que acontece)
            LogDispatcher {
                min_level: LogLevel::Info,
                format: LogFormat::Text,
                target: LogTarget::Console,
            },
            
            // 2. HTTP JSON - Escutando apenas ERROS para um serviço de monitoramento JSON
            LogDispatcher {
                min_level: LogLevel::Error,
                format: LogFormat::Json,
                target: LogTarget::Http { 
                    host_port: "127.0.0.1:9000".to_string(), // Onde estaria seu servidor JSON (ex: Splunk)
                    path: "/api/logs/erros".to_string(), 
                },
            },

            // 3. HTTP XML - Escutando WARN e ERROR para um sistema corporativo legado (SOAP/XML)
            LogDispatcher {
                min_level: LogLevel::Warn,
                format: LogFormat::Xml,
                target: LogTarget::Http { 
                    host_port: "127.0.0.1:9001".to_string(), // Onde estaria seu servidor XML
                    path: "/ws/logger".to_string(), 
                },
            }
        ])
    );

    server.add_route(HttpMethod::GET, "/erro", handler_simular_erros);

    // Simulando o disparo de eventos de todos os níveis
    server.logger.info("Servidor HTTP subiu na porta 8085 (Aparece só na tela).");
    server.logger.warn("Uso de memória alto detectado (Vai para Tela e para HTTP XML).");
    server.logger.error("Falha ao conectar no banco (Vai para Tela, HTTP XML e HTTP JSON).");

    server.run();
}
