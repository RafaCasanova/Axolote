extern crate axolote;
use axolote::Server;
use axolote::logger::{Logger, LoggerConfig, LogFormat, LogTarget, LogLevel, LogDispatcher};
use std::thread;
use std::time::Duration;

fn main() {
    println!("Iniciando exemplo 06 - Logger Standalone (Sem Servidor HTTP)...");

    // Instanciando o Logger DE FORMA INDEPENDENTE, sem criar um Server!
    // Ele vira uma ferramenta de telemetria para qualquer script,
    // worker ou daemon em Rust.
    let logger = Logger::new(LoggerConfig::new(vec![
            LogDispatcher {
                min_level: LogLevel::Info,
                format: LogFormat::Text,
                target: LogTarget::Console,
            },
            LogDispatcher {
                min_level: LogLevel::Error,
                format: LogFormat::Json,
                target: LogTarget::LocalFile { 
                    dir_path: "logs".to_string(), 
                    prefix: "meu_robo".to_string(),
                    retention_days: 2 
                },
            }
        ])
    );

    logger.info("Bot de processamento iniciado!");

    // Vamos simular um robô cuspindo log
    for i in 1..=3 {
        logger.info(&format!("Processando arquivo {}/3...", i));
        thread::sleep(Duration::from_secs(1));

        if i == 2 {
            // Gerando um log proposital de erro na regra de negócio
            logger.warn("O arquivo 2 estava corrompido, tentando reparar...");
            thread::sleep(Duration::from_secs(1));
            logger.error("Falha fatal ao processar o arquivo 2! Arquivo ignorado.");
        }
    }

    logger.info("Processamento finalizado com sucesso!");

    // Como o logger roda numa thread em background, damos um pequeno sleep
    // no final para garantir que as mensagens pendentes no canal sejam escritas
    // antes que o programa principal morra. (No Server HTTP isso não é preciso
    // porque o servidor roda pra sempre num loop infinito).
    thread::sleep(Duration::from_millis(50));
}
