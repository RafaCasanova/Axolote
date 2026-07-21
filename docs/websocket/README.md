# Documentação do Módulo WebSocket

O módulo WebSocket do `axolote` fornece uma implementação robusta e integral da RFC 6455 (WebSocket Protocol). Assim como o restante do framework, este módulo não possui dependências externas, processando handshakes, máscaras criptográficas (XOR) e frames binários estritamente via operações nativas.

## 1. Topologia e Gerenciamento de Estado

O sistema baseia-se num paradigma orientado a Hub (`WsHub`), que atua como centralizador de estado O(1) e roteador de mensagens entre conexões simultâneas. 

Diferente de implementações triviais que utilizam Mutexes globais, o `WsHub` utiliza um sistema de particionamento (Sharding) interno, reduzindo drasticamente a contenção (lock contention) e provendo vazão de dados para milhares de conexões paralelas.

## 2. Implementação e Roteamento de Handlers

Rotas WebSocket são vinculadas à estrutura principal do servidor utilizando a função `add_ws_route`. 

Diferentemente do modelo Request-Response, a comunicação WebSocket é mantida por um ciclo de vida perene no formato `fn(WsConnection, WsHub)`. O manipulador monopoliza a thread enquanto a conexão permanecer ativa.

```rust
extern crate axolote;
use axolote::prelude::*;

fn main() {
    let mut server = Server::new("8080");
    
    // O modo "WsMode::Both" permite que a rota funcione tanto para 
    // Upgrade via navegador quanto para requisições S2S locais.
    server.add_ws_route("/ws", WsMode::Both, chat_handler);
    
    server.run();
}
```

O ciclo de vida do manipulador baseia-se na extração serializada de pacotes de mensagem através da instrução iterativa `conn.receive()`. A leitura bloqueia a thread passivamente sem consumo residual de CPU até a chegada do próximo *frame*.

```rust
fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    // 1. Fase de Conexão e Inicialização
    let id_conexao = conn.id();
    conn.join("lobby");
    
    // 2. Loop de Recepção de Dados
    while let Some(msg) = conn.receive() {
        match msg {
            WsMessage::Text(texto) => {
                hub.broadcast_to_room("lobby", &format!("User {} disse: {}", id_conexao, texto));
            }
            WsMessage::Binary(dados) => {
                // Manipulação de pacotes binários customizados
            }
        }
    }
    
    // 3. Fase de Desconexão (Cleanup)
    // conn.receive() retornará None caso o túnel TCP encerre (Graceful close, Timeout ou Drop)
}
```

## 3. Gestão de Salas e Broadcast (Pub/Sub)

A distribuição de mensagens para múltiplos clientes é governada pelas Salas Virtuais (Rooms). 

Salas são abstrações lógicas instanciadas sob demanda pelo Hub, possuindo zero custo inicial. Conexões atrelam-se ativamente às salas através dos comandos de inscrição `join()` e cancelamento `leave()`.

```rust
// Adição à sala
conn.join("sala_administrativa");

// Propagação de mensagem à sala (Broadcast)
hub.broadcast_to_room("sala_administrativa", "Aviso geral do sistema!");

// Propagação suprimindo a emissão para o próprio remetente (Echo Suppression)
hub.broadcast_to_room_except("sala_administrativa", conn.id(), "Aviso para os demais.");
```

## 4. Metadados e Persistência Intra-sessão

Para viabilizar transações autenticadas ou manutenção de estado de domínio (e.g. nomes de usuário, níveis de acesso, tokens JWT decodificados), a estrutura `WsConnection` expõe um mapa nativo de metadados.

```rust
// Injeção de metadados após evento lógico (ex: Comando /nick)
conn.set_metadata("username", "admin_master");

// Extração subsequente
if let Some(user) = conn.get_metadata("username") {
    // ...
}
```

## 5. Arquitetura Distribuída e Clusterização (S2S)

O módulo provê capacidades avançadas de interconexão topológica (Node Mesh Cluster). Quando o modo cluster está ativado, a comunicação e o roteamento das salas rompem as barreiras do servidor local, espalhando-se transparentemente entre todos os nós instanciados numa arquitetura distribuída.

A expansão de escalabilidade horizontal baseada em Protocolo Gossip (Malha Inundada), bem como seu cache estrutural anti-loop e estratégias de eleição de liderança, estão detalhados no guia dedicado.

**Leia a Especificação Completa do Cluster:** [Documentação de Cluster (CLUSTER.md)](CLUSTER.md)

## 6. Módulo de Segurança (Handshake Security)

O módulo WebSocket possui um escudo de proteção atrelado à fase de *Upgrade HTTP*, permitindo barrar conexões maliciosas via validação estrita da RFC, mitigação de CSWSH (Verificação de Origin) e sistemas dinâmicos de autenticação via Token (Header e Query String).

**Aprenda a proteger suas rotas WS:** [Documentação de Segurança (SECURITY.md)](SECURITY.md)
