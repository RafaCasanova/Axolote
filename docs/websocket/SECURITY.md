# Segurança e Handshake no WebSocket (`WsSecurityGuard`)

O módulo `WsSecurityGuard` é um interceptador nativo projetado para avaliar e validar requisições antes de realizar o "Upgrade" de HTTP para WebSocket.
Ele permite barrar conexões maliciosas, não autorizadas ou incompatíveis logo na entrada, sem alocar as threads de *Workers* ou instanciar estruturas `WsConnection`.

## Principais Mecanismos de Proteção

1. **Strict RFC 6455 (`strict_rfc`)**:
   Por padrão, o servidor recusa qualquer tentativa de *Upgrade* que não utilize o método `GET` ou que não inclua o cabeçalho obrigatório `Sec-WebSocket-Version: 13`. Previne ataques de *Fuzzing* direcionados ao parser.

2. **Prevenção CSWSH (`allowed_origins`)**:
   Mitiga ataques de *Cross-Site WebSocket Hijacking* validando ativamente o cabeçalho `Origin`. Se o cliente for um browser e tentar conectar de um domínio não homologado na *whitelist*, receberá um `403 Forbidden`.

3. **Negociação de Subprotocolos (`allowed_subprotocols`)**:
   O servidor consegue forçar clientes a declararem capacidades específicas através do cabeçalho `Sec-WebSocket-Protocol`. Caso não enviem, recebem `400 Bad Request`.

4. **Autenticação Precoce (Tokens)**:
   Em vez de aceitar a conexão e pedir a senha por meio de um frame WebSocket, o `WsSecurityGuard` pode ser instruído a buscar:
   - Tokens em Query String (Ex: `ws://localhost/chat?api_key=XYZ`)
   - Tokens em Cabeçalhos (Ex: `Authorization: Bearer XYZ`)

5. **Validador Customizado Dinâmico (`custom_validator`)**:
   Um gancho (hook) poderoso. Você pode passar uma *closure* que inspeciona a estrutura `HttpRequest` original por completo, podendo extrair *Cookies*, validar JWTs na mão, ou verificar IPs através dos cabeçalhos de *Proxy* (ex: `X-Forwarded-For`).

---

## Exemplo de Uso (Configurando a Proteção)

Para proteger uma rota, você constrói as regras no `WsSecurityGuard` utilizando os métodos padrão e o empacota na sua `WsRouteConfig`.

```rust
extern crate axolote;
use axolote::prelude::*;
use std::sync::Arc;
use axolote::ws::security::WsSecurityGuard;

fn chat_seguro_handler(mut conn: WsConnection, _hub: WsHub) {
    conn.send("Autenticação validada pelo Handshake!");
    while let Some(_) = conn.receive() {
        // ...
    }
}

fn main() {
    let mut server = Server::new("8080");

    // 1. Instanciando as defesas do módulo de segurança
    let security = WsSecurityGuard::new()
        // Limita os domínios que podem se conectar (CSWSH Protection)
        .with_origins(vec!["https://meu-painel.com", "http://localhost:3000"])
        // Exige autenticação via API Key na URL
        .with_query_token("api_key", "MINHA_SENHA_MUITO_FORTE")
        // Exige um Token Customizado via Header
        .with_header_token("X-Admin-Token", "12345678")
        // Adiciona um validador manual customizado
        .with_validator(|req| {
            // Rejeita a conexão caso encontre um Cookie malicioso
            match req.headers.get("cookie") {
                Some(c) if c.contains("blocked_user=true") => false,
                _ => true,
            }
        });

    // 2. Anexando as regras de segurança à Configuração da Rota WS
    let mut config = WsRouteConfig::default();
    config.security = Some(Arc::new(security)); // Importante: Embrulhar em Arc

    // 3. Cadastrando a Rota no Servidor
    server.add_ws_route_with_config("/admin_chat", WsMode::Both, config, chat_seguro_handler);

    server.run();
}
```

O código nativo do servidor lidará com os cenários gerando os respectivos retornos HTTP (400 Bad Request, 401 Unauthorized, 403 Forbidden) garantindo extrema segurança para seus túneis de conexão assíncrona.
