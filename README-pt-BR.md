# Framework Axolote

[![Language](https://img.shields.io/badge/Language-Rust-orange.svg)](https://rust-lang.org)
[![Dependencies](https://img.shields.io/badge/Dependencies-0-brightgreen.svg)]()
[![Cargo](https://img.shields.io/badge/Cargo-Not_Required-blue.svg)]()
[![License](https://img.shields.io/badge/License-MIT-green.svg)](LICENSE)

*[English](README.md) | Português*

**Axolote** é um framework HTTP e WebSocket escrito em Rust sem dependências externas (sem o uso de Cargo ou crates de terceiros).

## Como é o código?

```rust
extern crate axolote;
use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn main() {
    let mut server = Server::new("8080");
    
    server.add_route(HttpMethod::GET, "/", |_req: HttpRequest| {
        HttpResponse::ok("Olá Mundo via Axolote!")
    });

    server.run();
}
```

Toda a fundação técnica — roteamento, análise de requisições, logging, parsing, concorrência e o protocolo WebSocket (incluindo clustering S2S) — foi escrita utilizando estritamente a biblioteca padrão do Rust (`std`).

## Arquitetura e Funcionalidades

O projeto é dividido em dois módulos principais. Consulte a documentação específica de cada módulo para exemplos e detalhes de implementação:

- **[Documentação do Módulo HTTP](docs/http/README.md)**: Detalha o sistema de roteamento dinâmico, extração de parâmetros de Path/Query, grupos de rotas, middlewares, concorrência nativa, tratamento de erros e suporte a IPv6.
- **[Documentação do Módulo WebSocket](docs/websocket/README.md)**: Detalha o protocolo RFC 6455 implementado do zero, gerenciamento de conexões assíncronas, sharding O(1) com Hub de comunicação, salas de transmissão e arquitetura de Cluster Descentralizado (S2S) com protocolo de propagação em malha (Gossip Mesh Relay).

## Instruções de Compilação

O projeto foi projetado para compilação direta via linha de comando (invocando o `rustc` nativamente) através de um arquivo `Makefile`.

### 1. Compilando o Core do Framework
Para compilar a engine e gerar a biblioteca estática `.rlib`:
```bash
make build-lib
```

### 2. Compilando os Exemplos
A pasta `/examples` contém instâncias práticas de como consumir o framework. Os exemplos estão separados em `/examples/http` e `/examples/websocket`.
Para compilar todos eles automaticamente:
```bash
make examples
```

### 3. Limpeza do Ambiente
Para remover arquivos de compilação, executáveis gerados e logs de teste, execute:
```bash
make clean
```

## Como Usar a Biblioteca em Outros Projetos

Como o projeto é construído sem dependências e sem o gerenciador de pacotes `cargo`, o resultado da compilação é um binário estático de biblioteca do Rust (`.rlib`).

Para utilizar o framework `axolote` no seu próprio projeto, siga os passos:

1. Baixe o arquivo gerado `libaxolote.rlib` e copie para dentro da pasta do seu projeto.
2. No seu código fonte (ex: `main.rs`), declare o uso da biblioteca:
   ```rust
   extern crate axolote;
   ```
3. Compile o seu projeto utilizando o compilador puro `rustc` passando a flag `--extern` para indicar onde a biblioteca se encontra:
   ```bash
   rustc main.rs --extern axolote=libaxolote.rlib
   ```

**Uso com Cargo (Para Usuários do Gerenciador de Pacotes):**  
Se o seu projeto de destino utilizar o Cargo, você pode automatizar a linkagem sem precisar passar flags no terminal. Para isso:
1. Copie o arquivo `libaxolote.rlib` e também o arquivo **`build.rs`** deste repositório para a raiz do seu projeto Cargo (ao lado do seu `Cargo.toml`).
2. Adicione `extern crate axolote;` no seu `main.rs`.
3. Rode `cargo build` ou `cargo run` normalmente. O arquivo `build.rs` instruirá o Cargo a encontrar e anexar a biblioteca pré-compilada automaticamente.

## Exemplos e Documentação Detalhada

Para conferir guias completos de arquitetura, tutoriais passo a passo e códigos de exemplo prontos para execução, consulte os diretórios dentro da pasta [`docs/`](docs/):

- **Módulo HTTP**:
  - [Guia e Documentação HTTP](docs/http/README.md)
  - [Exemplos de Código HTTP](docs/http/examples/)
- **Módulo WebSocket**:
  - [Guia e Documentação WebSocket](docs/websocket/README.md)
  - [Módulo de Segurança WebSocket](docs/websocket/SECURITY.md)
  - [Arquitetura de Cluster e Gossip Mesh](docs/websocket/CLUSTER.md)
  - [Exemplos de Código WebSocket](docs/websocket/examples/)

## Licença

Distribuído sob a licença MIT. Consulte o arquivo `LICENSE` para maiores informações e restrições legais.
