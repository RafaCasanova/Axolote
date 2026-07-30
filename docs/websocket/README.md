# Documentação do Módulo WebSocket

O módulo WebSocket do `axolote` fornece a implementação da RFC 6455 (WebSocket Protocol). Este módulo não possui dependências externas, processando handshakes, máscaras criptográficas (XOR) e frames binários via operações da biblioteca padrão.

## 1. Topologia e Gerenciamento de Estado

O sistema baseia-se num paradigma orientado a Hub (`WsHub`), que atua como centralizador de estado O(1) e roteador de mensagens entre conexões simultâneas. 

O `WsHub` utiliza um sistema de particionamento (Sharding) interno para redução de contenção (lock contention) entre conexões paralelas.

## 2. Implementação e Roteamento de Handlers

Rotas WebSocket são vinculadas à estrutura principal do servidor utilizando a função `add_ws_route`. 

A comunicação WebSocket é mantida por um ciclo de vida no formato `fn(WsConnection, WsHub)`. O manipulador executa enquanto a conexão permanecer ativa.

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

O ciclo de vida do manipulador baseia-se na configuração de callbacks de mensagem e fechamento através das instruções `conn.on_message()` e `conn.on_close()`. O processamento real das mensagens ocorre de forma assíncrona no motor (Reactor).

```rust
fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    // 1. Fase de Conexão e Inicialização
    conn.join("lobby");
    
    // 2. Recepção de Dados (Callback Assíncrono)
    conn.on_message(move |id, hub, msg| {
        match msg {
            WsMessage::Text(texto) => {
                hub.broadcast_to_room("lobby", &format!("User {} disse: {}", id, texto));
            }
            WsMessage::Binary(dados) => {
                // Manipulação de pacotes binários customizados
            }
            _ => {} // Ignora Ping, Pong e Close
        }
    });
    
    // 3. Fase de Desconexão (Cleanup Callback)
    conn.on_close(move |id, hub, _code| {
        hub.broadcast_to_room("lobby", &format!("User {} saiu da sala.", id));
    });
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

### 3.1. Limpeza Automática de Salas Vazias (Room Cleanup)

Quando salas são criadas dinamicamente (ex: partidas de jogo, sessões temporárias), elas podem acumular entradas órfãs em memória após todos os clientes desconectarem. O Hub possui um mecanismo nativo de limpeza que remove automaticamente as salas sem clientes.

Por padrão, a limpeza roda **a cada 60 segundos**. O comportamento é totalmente configurável:

```rust
// Alterar o intervalo de limpeza para 120 segundos
server.set_ws_room_cleanup_interval(120);

// Desabilitar a limpeza automática (controle manual)
server.disable_ws_room_cleanup();

// Executar a limpeza manualmente a qualquer momento
// (pode ser chamado de um handler HTTP de administração, por exemplo)
server.ws_cleanup_rooms();
```

A limpeza também pode ser invocada diretamente pelo `WsHub` dentro de um handler WebSocket:

```rust
fn admin_handler(mut conn: WsConnection, hub: WsHub) {
    conn.on_message(move |id, hub, msg| {
        if let WsMessage::Text(cmd) = msg {
            if cmd == "/cleanup" {
                hub.cleanup_empty_rooms();
                hub.send_to(id, "Salas vazias removidas da memoria.");
            }
        }
    });
}
```

Além da varredura periódica, o Hub também limpa salas individualmente em tempo real: quando um cliente desconecta (`unregister`) ou sai de uma sala (`leave`), o sistema verifica se a sala ficou sem clientes locais e a remove imediatamente das estruturas internas do cluster (`local_rooms` e `room_leaders`).

## 5. Arquitetura Distribuída e Clusterização (S2S)

O módulo provê suporte a interconexão topológica (Node Mesh Cluster). Quando o modo cluster está ativado, a comunicação e o roteamento de salas são propagados entre os nós conectados.

A expansão em malha baseada no protocolo Gossip, bem como o cache de deduplicação e controle de liderança, estão detalhados no guia dedicado.

**Leia a Especificação Completa do Cluster:** [Documentação de Cluster (CLUSTER.md)](CLUSTER.md)

## 6. Módulo de Segurança (Handshake Security)

O módulo WebSocket possui um sistema de validação atrelado à fase de *Upgrade HTTP*, permitindo checar requisições via validação da RFC, verificação de Origin (CSWSH) e autenticação via Token (Header e Query String).

**Aprenda a proteger suas rotas WS:** [Documentação de Segurança (SECURITY.md)](SECURITY.md)

## 7. Referência Rápida de API (Cheatsheet)

Abaixo estão utilitários e funções avançadas que compõem a interface pública do Módulo WebSocket, mas que geralmente não aparecem nos fluxos básicos:

### `WsConnection` (Instância Individual de Conexão)
- `conn.send(msg: &str) -> bool`: Envia uma mensagem em texto bruto *diretamente* à conexão invocadora, pulando a camada de roteamento de salas do `WsHub`.
- `conn.send_json<T>(data: &T) -> bool`: Serializa automaticamente a Struct fornecida para JSON e envia via frame de texto.
- `conn.on_message_json<T, F>(cb: F)`: Registra um callback que recebe os dados já convertidos a partir de JSON diretamente na _Struct_ Rust (requer trait `FromJson`). A conexão descarta e ignora _frames_ inválidos nativamente.
- `conn.close()`: Permite que a camada servidora force um fechamento gracioso ativo (envia o _opcode_ Close e desconecta).
- `conn.change_id(new_id: u64)`: Altera a matrícula de identificação única do nó Socket durante o ciclo de vida da conexão. Crucial para atrelar a sessão a um ID de banco de dados após a conclusão de uma verificação de login assíncrona.

### `WsHub` (Controlador Global O(1))
- `hub.broadcast(msg: &str)`: Emissão massiva. Dispara a mensagem instantaneamente para **absolutamente todos** os clientes online no servidor, independentemente de estarem em salas.
- `hub.broadcast_except(exclude_id, msg)`: Mesma mecânica do `broadcast`, porém ignorando um Socket específico (Supressão de Eco).
- `hub.broadcast_json(...)`, `hub.broadcast_json_to_room(...)`: Família inteira de métodos espelhada com suporte nativo a serialização de pacotes `Struct -> JSON`.
- `hub.kick(id: u64)`: Ferramenta administrativa; desliga e força a desconexão de um usuário específico em qualquer lugar do motor (ideal para comandos de Banning e Kicks de moderação).
- `hub.count()` e `hub.room_count(room: &str)`: Retornam contadores processuais instantâneos. Útil para exibição de métricas como "1540 jogadores online" ou "32 na sala Lobby".
- `hub.set_client_metadata(id, key, value)` / `hub.get_client_metadata(id, key)`: Diferente do `conn.set_metadata` (que funciona no próprio handler da conexão), estes métodos permitem que handlers externos de um usuário editem os metadados de **outro usuário**. (Ex: Administrador rodando o comando `/mutar 55`, que marca a variável `mutado=true` no Socket ID 55).
