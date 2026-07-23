# Documentação do Módulo HTTP

O módulo HTTP do `axolote` provê roteamento e tratamento de requisições sem dependências externas. A estrutura baseia-se na thread pool do sistema operacional, processando conexões de forma concorrente.

## 1. Instanciação e Concorrência

O núcleo da aplicação HTTP é a estrutura `Server`. Ao iniciar, ela reserva a porta especificada e, para cada conexão TCP estabelecida, delega a leitura e escrita do stream para uma nova thread independente. 

```rust
extern crate axolote;

use axolote::Server;
use axolote::http::{HttpMethod, HttpRequest, HttpResponse};

fn main() {
    let mut server = Server::new("8080");
    server.run(); // Bloqueia a thread principal ouvindo conexões
}
```

Internamente, instâncias da configuração de rotas são propagadas via `Arc` (Atomic Reference Counting) para permitir acesso paralelo seguro.

## 2. Roteamento e Manipuladores de Requisição

A API de roteamento permite associar métodos HTTP específicos a manipuladores de função (Handlers). 

Um manipulador deve respeitar a assinatura `fn(HttpRequest) -> HttpResponse`.

### Roteamento Simples
```rust
fn home_handler(_req: HttpRequest) -> HttpResponse {
    HttpResponse::ok("Bem-vindo à página inicial!")
}

server.add_route(HttpMethod::GET, "/", home_handler);
```

### Roteamento com Parâmetros Dinâmicos de Caminho (Path Parameters)
Variáveis no caminho da URL são declaradas entre chaves `{}`. O engine de roteamento realiza o *pattern matching* e injeta o valor analisado no mapa `req.params`.

```rust
fn user_profile(mut req: HttpRequest) -> HttpResponse {
    if let Some(user_id) = req.params.remove("id") {
        HttpResponse::ok(&format!("Perfil do Usuário: {}", user_id))
    } else {
        HttpResponse::bad_request("ID inválido")
    }
}

// Rota registrada: /user/1234
server.add_route(HttpMethod::GET, "/user/{id}", user_profile);
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
protected_group.add_middleware(auth_middleware);
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
custom_response.headers.insert("Content-Type".to_string(), "application/json".to_string());
```

## 6. Sistema de Registro (Logger)

Um módulo nativo de logging foi introduzido para gerar rastreabilidade estruturada. Ele calcula o offset de *Unix Epoch* em `SystemTime` para padronizar carimbos temporais, registrando eventos de roteamento e execuções em três níveis de prioridade.

```rust
use axolote::server::logger;

logger::log_info("Servidor HTTP inicializado com sucesso.");
logger::log_warn("Alta latência detectada na requisição de banco de dados.");
logger::log_error("Falha ao analisar o corpo JSON.");
```

## 7. Suporte a JSON e Serialização

A macro `#[axolote_json]` permite serializar e desserializar estruturas JSON automaticamente, sem adicionar sujeira ao sistema HTTP principal.

```rust
use axolote::json::{FromJson, ToJson};

#[axolote_json]
struct User {
    name: String,
    age: u8,
    
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
    let user = match User::from_json(&req.body) {
        Ok(u) => u,
        Err(e) => return HttpResponse::bad_request(e),
    };

    HttpResponse::ok(user.to_json())
        .with_header("Content-Type", "application/json")
}

fn handle_bind(req: HttpRequest) -> HttpResponse {
    // ABORDAGEM 2: bind_json (Preenchimento Mutável - Atualiza só o que vier no JSON)
    let mut user = User {
        name: "Desconhecido".to_string(),
        age: 0,
    };
    
    // Atualiza a estrutura com os dados recebidos. Pode gerar erro se o payload for inválido.
    if let Err(e) = user.bind_json(&req.body) {
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

// Exemplo retornando uma Struct (JSON automático)
fn user_profile_handler(_req: HttpRequest) -> HttpResponse {
    let user = User {
        id: 123,
        username: "Rafael".to_string(),
        internal_hash: "oculto".to_string(),
    };

    // A trait IntoBody converte a struct automaticamente para JSON 
    // e injeta o cabeçalho Content-Type: application/json
    HttpResponse::ok(user)
        .with_header("X-Powered-By", "Axolote")
}
```
