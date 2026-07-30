# Documentação do Módulo HTTP

O módulo HTTP do `axolote` provê roteamento e tratamento de requisições sem dependências externas. A arquitetura baseia-se em um **Reactor Event-Loop** (`epoll` nativo no Linux) aliado a um **ThreadPool**. Isso permite um processamento de conexões não-bloqueante (asynchronous I/O) de altíssima performance, escalando para dezenas de milhares de conexões simultâneas (C10K problem).

## 1. Funcionamento do Servidor

O núcleo da aplicação HTTP é a estrutura `Server`. Ao iniciar, ela instancia um Reactor (`epoll`) em uma thread dedicada e cria um pool de threads (calculado com base nos núcleos lógicos disponíveis). Quando uma conexão TCP chega, ela é configurada como não-bloqueante (`O_NONBLOCK`) e registrada no Reactor. Toda leitura e escrita subsequentes disparam eventos que são delegados aos *Workers* da ThreadPool, liberando o Reactor para aceitar mais conexões sem travar. 

```rust
extern crate axolote;

use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn main() {
    let mut server = Server::new("8080");

    // Para expor na rede local ou internet, altere o host para "0.0.0.0"
    server.set_host("0.0.0.0");

    server.run(); // Bloqueia a thread principal ouvindo conexões
}
```

Internamente, instâncias da configuração de rotas são propagadas via `Arc` (Atomic Reference Counting) para permitir acesso paralelo seguro.

## 2. Roteamento e Manipuladores de Requisição

A API de roteamento permite associar métodos HTTP específicos a manipuladores de função ou closures (`Box<dyn Fn(HttpRequest) -> HttpResponse + Send + Sync>`). 

Um manipulador aceita um `HttpRequest` e retorna um `HttpResponse`. Podem ser usadas tanto funções normais quanto closures `move || {}`:

### Roteamento Simples
```rust
fn home_handler(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Bem-vindo à página inicial!")
}

server.add_route(HttpMethod::GET, "/", home_handler);
```

### CORS Nativo (Cross-Origin Resource Sharing)
Para plugar a sua API HTTP num Frontend feito em React, Vue ou Angular, você precisa lidar com CORS. 
O Axolote possui um Middleware Nativo embutido no núcleo do servidor que resolve isso com 1 linha de código e trata requisições de *preflight* (`OPTIONS`) de forma invisível.

```rust
use axolote::http::cors::CorsConfig;

// Habilita CORS com a configuração padrão (Libera tudo '*')
server.enable_cors(CorsConfig::default());

// Ou, de forma estrita para Produção:
// server.enable_cors(CorsConfig::restrictive(vec!["https://meu-app.com", "http://localhost:3000"]));
```

### Servidor de Arquivos Estáticos (HTML, CSS, JS, Imagens)
Se você for hospedar o Frontend (React, Vue, etc) no mesmo servidor ou quiser expor imagens, PDFs e vídeos, o Axolote serve os binários nativamente inferindo Content-Types sem necessidade de plugins:

```rust
// Mapeia as requisições que começam com "/public" para a pasta local "./meus_arquivos"
// Proteção automática contra Path Traversal inclusa!
server.serve_dir("/public", "./meus_arquivos");
```

### Roteamento com Parâmetros Dinâmicos de Caminho e Tipagem (Radix Tree)
Variáveis no caminho da URL são declaradas entre chaves `{}`. O engine de roteamento agora utiliza uma estrutura de **Radix Tree (Trie)** de alta performance, permitindo resolver caminhos em tempo $O(K)$ independente do número de rotas cadastradas. 

O sistema suporta sufixos de tipagem para restringir valores e resolver conflitos de mesma estrutura (Backtracking):
- `{variavel:num}`: Aceita apenas números (ex: `123`).
- `{variavel:alpha}`: Aceita apenas letras (ex: `bob`).
- `{variavel:alnum}`: Aceita letras e números (Alfanumérico).
- `{variavel}`: Aceita qualquer segmento (Fallback universal).

O *pattern matching* injeta o valor validado no mapa `req.params`.

```rust
fn user_profile(mut req: HttpRequest) -> HttpResponse {
    if let Some(user_id) = req.params.remove("id") {
        HttpResponse::ok(&format!("Perfil do Usuário: {}", user_id))
    } else {
        HttpResponse::bad_request("ID inválido")
    }
}

// Rota registrada: /user/1234
server.add_route(HttpMethod::GET, "/user/{id:num}", user_profile);
// Pode co-existir perfeitamente com /user/{name:alpha} sem colisões!
```

### Parâmetros de Consulta (Query Parameters)
Argumentos posicionados após o caractere `?` na URI são automaticamente analisados e populados em `req.query_params`.

```rust
// Exemplo de URL: /search?query=rust&limit=10
fn search_handler(mut req: HttpRequest) -> HttpResponse {
    let termo = req.query_params.remove("query").unwrap_or_default();
    HttpResponse::ok(&format!("Buscando por: {}", termo))
}
```

## 3. Arquitetura em Múltiplas Camadas (Route Groups)

Aplicações complexas requerem versionamento ou isolamento lógico (ex: `/api/v1/` e `/api/v2/`). O `RouteGroup` permite encapsular rotas sob um mesmo escopo base.

```rust
let mut api_v1 = RouteGroup::new("/api/v1");
api_v1.add_route(HttpMethod::GET, "/status", status_handler);
api_v1.add_route(HttpMethod::POST, "/data", upload_handler);

server.add_group(api_v1); // Disponibiliza /api/v1/status
```

## 4. Sistema de Middlewares

Middlewares atuam como interceptadores no ciclo de vida de uma requisição e são acoplados primariamente a um `RouteGroup`. Eles são executados antes da requisição atingir o manipulador final.

A assinatura de um middleware exige que ele retorne um `Option<HttpResponse>`. Se o middleware retornar `Some`, a cadeia é interrompida, e a resposta é enviada imediatamente. Se retornar `None`, a requisição prossegue ao próximo estágio.

```rust
fn auth_middleware(req: &mut HttpRequest) -> Option<HttpResponse> {
    match req.headers.get("Authorization") {
        Some(token) if token == "SecretToken" => None, // Permite passagem
        _ => Some(HttpResponse::unauthorized("Acesso Negado: Token Inválido")), // Interrompe
    }
}

let mut protected_group = RouteGroup::new("/secure");
protected_group.set_middleware(auth_middleware);
protected_group.add_route(HttpMethod::GET, "/data", secure_data_handler);
```

## 5. Respostas e Tratamento de Erros

A estrutura `HttpResponse` encapsula o código de estado HTTP, cabeçalhos customizados e o corpo da resposta. A biblioteca oferece atalhos semânticos para as respostas mais convencionais:

- `HttpResponse::ok(body)` (200 OK)
- `HttpResponse::not_found(body)` (404 Not Found)
- `HttpResponse::bad_request(body)` (400 Bad Request)
- `HttpResponse::unauthorized(body)` (401 Unauthorized)
- `HttpResponse::internal_server_error(body)` (500 Internal Error)

Para construir respostas complexas, utilize o construtor customizado:
```rust
let mut custom_response = HttpResponse::new(201, "Created", "Recurso salvo com sucesso");
custom_response.headers.push(("Content-Type".to_string(), "application/json".to_string()));
```

## 6. Sistema de Registro (Logger)

Um módulo nativo de logging foi introduzido para gerar rastreabilidade estruturada. A instância do servidor possui um logger que pode ser utilizado:

```rust
server.logger.info("Servidor HTTP inicializado com sucesso.");
server.logger.warn("Alta latência detectada na requisição de banco de dados.");
server.logger.error("Falha ao analisar o corpo JSON.");
```

## 7. Suporte a JSON e Serialização

A macro `#[axolote_json]` permite serializar e desserializar estruturas JSON automaticamente, sem adicionar sujeira ao sistema HTTP principal.

```rust
use axolote::json::{FromJson, ToJson};

#[axolote_json]
struct User {
    id: u64,
    username: String,
    
    // Este campo não será convertido para JSON nem exibido para o cliente
    #[json(ignore)]
    pub internal_hash: String,
}
```

### Recebendo e Retornando JSON

```rust
fn handle_post(req: HttpRequest) -> HttpResponse {
    // ABORDAGEM 1: from_json (Cria a struct do zero lendo a string)
    // O retorno é Result<User, String>, você tem total controle!
    let user = match User::from_json(&req.body_utf8()) {
        Ok(u) => u,
        Err(e) => return HttpResponse::bad_request(e),
    };

    HttpResponse::ok(user.to_json())
        .with_header("Content-Type", "application/json")
}

fn handle_bind(req: HttpRequest) -> HttpResponse {
    // ABORDAGEM 2: bind_json (Preenchimento Mutável - Atualiza só o que vier no JSON)
    let mut user = User {
        id: 0,
        username: "Desconhecido".to_string(),
        internal_hash: "".to_string(),
    };
    
    // Atualiza a estrutura com os dados recebidos. Pode gerar erro se o payload for inválido.
    if let Err(e) = user.bind_json(&req.body_utf8()) {
        return HttpResponse::bad_request(&format!("Erro: {}", e));
    }

    HttpResponse::ok(user.to_json())
        .with_header("Content-Type", "application/json")
}
```

## 8. Form Data e URL Decoding

Para interpretar formulários HTML convencionais (`application/x-www-form-urlencoded`), a estrutura da requisição oferece um motor independente de decodificação de URL capaz de extrair os parâmetros com precisão.

O método `url_decode()` resolve nativamente a quebra de espaços baseada em `+` e processa formatações UTF-8 e cadeias hexadecimais (ex: `%20`). O framework trata isso em parâmetros de rota automaticamente, e provê o método auxiliar para corpos de requisição:

```rust
fn form_handler(req: HttpRequest) -> HttpResponse {
    // Retorna um HashMap<String, String> com as chaves e valores decodificados
    let form = req.form_data();
    
    if let Some(username) = form.get("username") {
        HttpResponse::ok(&format!("Recebido: {}", username))
    } else {
        HttpResponse::bad_request("Campo obrigatório ausente")
    }
}
```

## 9. Padrão Builder para Respostas HTTP

O ciclo de construção de uma `HttpResponse` adota o padrão de projeto Builder, entregando ergonomia e abstração para a formatação de dados.
Devido à Trait de identificação inteligente de corpo (`IntoBody`), construtores como `HttpResponse::ok(...)` aceitam fluidamente tanto strings puras quanto `Structs` JSON, formatando o cabeçalho `Content-Type` de forma condizente (ex: `text/plain` vs `application/json`).

Com isso, configurações de cabeçalhos estendidos e cookies podem ser processadas em cadeia na própria linha de resposta, aceitando **tanto texto puro quanto structs completas**:

```rust
// Exemplo retornando texto puro
fn auth_handler(_req: HttpRequest) -> HttpResponse {
    let auth_token = "ey...token_seguro";

    HttpResponse::ok("Autenticado com sucesso")
        .with_header("X-Custom-Auth", "Active")
        .with_cookie("session", auth_token)
}

// Exemplo retornando uma Struct (Transformando em JSON)
fn user_profile_handler(_req: HttpRequest) -> HttpResponse {
    let user = User {
        id: 123,
        username: "Rafael".to_string(),
        internal_hash: "oculto".to_string(),
    };

    // Converta a struct para string JSON manualmente
    HttpResponse::ok(user.to_json())
        .with_header("Content-Type", "application/json")
        .with_header("X-Powered-By", "Axolote")
}
```

## 10. Referência Rápida de API (Cheatsheet)

Além das funções principais demonstradas acima, o framework expõe nativamente os seguintes métodos utilitários:

### `Server`
- `Server::new_ipv6(addr: &str)`: Semelhante ao `new`, mas faz o _bind_ forçado em um endereço IPv6 (ex: `[::1]:8080`).
- `server.set_timeout(secs: u64)`: Altera o limite máximo de inatividade (timeout) de conexões HTTP (Padrão: 10 segundos). Conexões ociosas são expurgadas pelo _Reactor_ para poupar recursos.
- `server.set_not_found_handler(handler)`: Permite sobrescrever a página padrão `404 Not Found`. Útil para redirecionar todas as rotas não encontradas para o `index.html` em aplicações _Single Page Application_ (React, Vue).
- `Server::shutdown()`: Encera o Motor (_Reactor_) e a _ThreadPool_ graciosamente. Pode ser invocado para desligamento via sinais do SO.
- `server.get_ws_hub()`: Retorna um clone da instância global do `WsHub` para ser utilizada fora das rotas (ex: threads em background).

### `HttpRequest`
- `req.take_body()`: Drena e retorna o payload bruto da requisição no formato `Vec<u8>`. Necessário para lidar com recebimento de arquivos binários, imagens de formulários _Multipart_, ou dados não-texto.

### `HttpResponse`
- `HttpResponse::created(body)`: Atalho sintático para o código de resposta `201 Created`.
- `HttpResponse::redirect(location: &str)`: Atalho sintático para redirecionamento web, gera `302 Found` apontando o cabeçalho `Location` para a URL desejada.
- `response.with_cookie_secure(key, value, path, http_only, secure)`: Função avançada para emissão de Cookies protegidos, mitigando ataques _XSS_ (`http_only`) e _Man-in-the-Middle_ (`secure`).
