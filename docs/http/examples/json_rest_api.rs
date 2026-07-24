extern crate axolote;

use axolote::{Server, axolote_json};
use axolote::http::{HttpRequest, HttpResponse, HttpMethod};
use axolote::json::{FromJson, ToJson};

// A macro #[axolote_json] implementa automaticamente a serialização e
// desserialização de estruturas, gerando as implementações de ToJson e FromJson.
#[axolote_json]
struct CreateUserReq {
    name: String,
    age: u8,
}

#[axolote_json]
struct UserResponse {
    id: u64,
    name: String,
    age: u8,
}

// -----------------------------------------------------------------------------
// Utilização das propriedades "rename" e "ignore"
// -----------------------------------------------------------------------------
#[axolote_json]
struct UserProfile {
    #[json(rename = "identificador")]
    id: u64,

    name: String,

    #[json(ignore)]
    internal_secret: String,
}

// -----------------------------------------------------------------------------

fn create_user(req: HttpRequest) -> HttpResponse {
    // Desserializa a string JSON para a struct.
    // O retorno é Result<CreateUserReq, String>, exigindo tratamento explícito.
    let req_obj = match CreateUserReq::from_json(&req.body_utf8()) {
        Ok(obj) => obj,
        Err(e) => return HttpResponse::bad_request(e), // Se falhar, avisa o cliente imediatamente!
    };

    let resp = UserResponse {
        id: 12345, // ID gerado pelo sistema

        name: req_obj.name,
        age: req_obj.age,
    };

    println!("Usuário criado: {} (ID: {})", resp.name, resp.id);

    // Serializa a struct para JSON e a inclui no corpo da resposta HTTP
    HttpResponse::created(resp.to_json())
        .with_header("Content-Type", "application/json")
}

fn update_user(req: HttpRequest) -> HttpResponse {
    // Carrega dados pré-existentes do usuário (ex: de um banco de dados).
    let mut current_user = CreateUserReq {
        name: "Carlos Velho".to_string(),
        age: 60,
    };

    // Realiza o binding do JSON recebido, sobrepondo os valores na struct.
    // Se o formato geral for inválido, podemos retornar erro.
    if let Err(e) = current_user.bind_json(&req.body_utf8()) {
        return HttpResponse::bad_request(format!("JSON Inválido: {}", e));
    }

    HttpResponse::ok(current_user.to_json())
        .with_header("Content-Type", "application/json")
}

fn manual_conversion(_req: HttpRequest) -> HttpResponse {
    // Demonstração de uso direto das traits ToJson e FromJson geradas pela macro.
    let user = CreateUserReq { name: "Maria".to_string(), age: 25 };

    // Serializa a struct para uma string formatada em JSON
    let str_json = user.to_json(); 

    // Instancia a struct a partir da string formatada em JSON
    // Aqui usamos o unwrap_or_default() para simular o comportamento antigo de fallback
    let final_user = CreateUserReq::from_json(&str_json).unwrap_or_default();

    HttpResponse::ok(final_user.to_json())
        .with_header("Content-Type", "application/json")
}

fn advanced_mapping(_req: HttpRequest) -> HttpResponse {
    let profile = UserProfile {
        id: 99,
        name: "Alice".to_string(),
        internal_secret: "super_secret_xyz".to_string(),
    };

    // Serializa a estrutura respeitando as anotações.
    // O campo 'internal_secret' será omitido e 'id' será serializado como 'identificador'.
    HttpResponse::ok(profile.to_json())
        .with_header("Content-Type", "application/json")
}

fn main() {
    let mut server = Server::new("8080");

    // Adiciona as rotas de API JSON
    server.add_route(HttpMethod::POST, "/api/users", create_user);
    server.add_route(HttpMethod::PUT, "/api/users/update", update_user);
    server.add_route(HttpMethod::GET, "/api/users/manual", manual_conversion);
    server.add_route(HttpMethod::GET, "/api/users/advanced", advanced_mapping);

    println!("Servidor rodando em http://127.0.0.1:8080");
    println!("Para testar, envie um POST com JSON:");
    println!("curl -X POST http://127.0.0.1:8080/api/users -d '{{\"name\": \"Rafael\", \"age\": 35}}'");
    
    server.run();
}
