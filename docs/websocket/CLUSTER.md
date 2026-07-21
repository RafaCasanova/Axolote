# WebSocket Distribuido - Manual de Integracao e Configuraçao de Cluster

Este documento descreve a arquitetura, o protocolo S2S (Server-to-Server) e o guia de configuracao do cluster WebSocket distribuido do framework `axolote`.

A arquitetura de cluster e totalmente opcional (modo Standalone por padrao). Quando ativada, ela habilita a comunicacao Server-to-Server sem dependencias externas de terceiros (como Redis ou bancos de dados compartilhados).

---

## 1. Arquitetura do Cluster (Gossip Node Mesh)

O cluster baseia-se em uma topologia em malha (*Gossip Protocol*). Os servidores conectam-se utilizando sockets TCP S2S dedicados e retransmitem as informações entre os nós.

```
       ┌──────────────┐         (Relay)          ┌──────────────┐
       │   Servidor   │◄────────────────────────►│   Servidor   │
       │    Node 1    │      Porta S2S:9001      │    Node 2    │
       │ (HTTP: 8081) │                          │ (HTTP: 8082) │
       └──────┬───▲───┘                          └───▲───┬──────┘
              │   │                                  │   │
       ┌──────▼───┴───┐                      ┌───────┴───▼──┐
       │   Cliente A  │                      │   Servidor   │
       │ (WebSocket)  │                      │    Node 3    │
       └──────────────┘                      │ (HTTP: 8083) │
                                             └──────┬───────┘
                                                    │
                                             ┌──────▼───────┐
                                             │   Cliente B  │
                                             │ (WebSocket)  │
                                             └──────────────┘
```

### Mecanismos de Funcionamento

* **Roteamento em Malha (Gossip Relay):** Quando um servidor recebe uma mensagem inédita de um vizinho (ex: Node 1 para Node 2), ele processa a mensagem para seus clientes locais e retransmite-a para todos os seus outros vizinhos (ex: Node 2 para Node 3). Isso permite que as mensagens sejam propagadas pela rede entre os nós disponíveis.
* **Deduplicação de Mensagens Anti-Loop:** Como o protocolo Gossip propaga em todas as direções, mensagens podem cruzar os mesmos servidores múltiplas vezes. O *axolote* utiliza um *Cache LIFO de Deduplicação* O(1), mantendo a assinatura das últimas 100.000 mensagens recebidas em memória `(Node Original, Número Sequencial)` para ignorar mensagens duplicadas.
* **Leader-per-Room (Líder por Sala):** Cada sala criada dinamicamente no WebSocket possui um nó "líder". O primeiro nó que registra a presença local em uma sala é eleito o líder dela, facilitando rotas.
* **Autodiscovery & Heartbeat:** Os nós utilizam heartbeats periódicos (Ping/Pong S2S) para validar a conexão direta. Se um nó for detectado como morto, os demais reelegem a liderança das salas afetadas e fecham o peer defeituoso.

---

## 2. API de Configuraçao

O cluster e configurado usando a struct `ClusterConfig` e inicializado atraves do metodo `enable_cluster()` na struct `Server`.

### ClusterConfig

```rust
pub struct ClusterConfig {
    pub node_id: u8,
    pub s2s_port: String,
    pub seed_nodes: Vec<String>,
    pub heartbeat_interval_secs: u64,
    pub heartbeat_missed_limit: u32,
    pub max_seen_messages_cache: usize,
}
```

#### Parametros de Configuraçao:

1. **`node_id` (u8):** Identificador unico do nó dentro do cluster. Cada servidor rodando na malha deve obrigatoriamente possuir um ID distinto (ex: 1, 2, 3...).
2. **`s2s_port` (String/&str):** Porta TCP em que o nó local ira escutar as conexoes de outros servidores (S2S).
3. **`seed_nodes` (Vec<String>):** Lista de enderecos de outros nos conhecidos na rede (`IP:Porta_S2S`) aos quais este nó tentara se conectar ativamente durante a inicializacao.
4. **`heartbeat_interval_secs` (u64):** Intervalo em segundos em que o nó envia pacotes de batimento cardiaco (Ping) para os peers conectados. Padrao recomendavel: `5`.
5. **`heartbeat_missed_limit` (u32):** Numero maximo de batimentos cardiacos perdidos sucessivamente antes de declarar a conexao com o peer remetente como morta e forcar desconexao. Padrao: `3`.
6. **`max_seen_messages_cache` (usize):** Tamanho do cache de deduplicacao. Padrao: `100_000`.

---

## 3. Exemplo Pratico de Configuraçao

Para rodar dois servidores conectados em cluster na mesma maquina, configure-os com IDs e portas distintos e aponte um nó ao outro.

### Codigo do Servidor - Node 1

```rust
extern crate axolote;
use axolote::prelude::*;

fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    conn.join("lobby");
    
    while let Some(msg) = conn.receive() {
        if let WsMessage::Text(texto) = msg {
            // Este broadcast enviara a mensagem para todos os usuarios no "lobby",
            // independente do nó do cluster em que eles estejam conectados.
            hub.broadcast_to_room("lobby", &format!("Node 1 [User {}]: {}", conn.id(), texto));
        }
    }
}

fn main() {
    // Porta HTTP publica de clientes
    let mut server = Server::new("8081");
    
    // Configura o Node 1
    // ID: 1
    // Porta de escuta S2S: 9001
    // Seed nodes: aponta para a porta S2S do Node 2 (9002)
    let config = ClusterConfig::new(
        1, 
        "9001", 
        vec!["127.0.0.1:9002".to_string()]
    );
    
    // Habilita o modo cluster
    server.enable_cluster(config);
    
    server.add_ws_route("/chat", WsMode::Both, chat_handler);
    server.run();
}
```

### Codigo do Servidor - Node 2

```rust
extern crate axolote;
use axolote::prelude::*;

fn chat_handler(mut conn: WsConnection, hub: WsHub) {
    conn.join("lobby");
    
    while let Some(msg) = conn.receive() {
        if let WsMessage::Text(texto) = msg {
            hub.broadcast_to_room("lobby", &format!("Node 2 [User {}]: {}", conn.id(), texto));
        }
    }
}

fn main() {
    // Porta HTTP publica de clientes
    let mut server = Server::new("8082");
    
    // Configura o Node 2
    // ID: 2
    // Porta de escuta S2S: 9002
    // Seed nodes: aponta para a porta S2S do Node 1 (9001)
    let config = ClusterConfig::new(
        2, 
        "9002", 
        vec!["127.0.0.1:9001".to_string()]
    );
    
    server.enable_cluster(config);
    
    server.add_ws_route("/chat", WsMode::Both, chat_handler);
    server.run();
}
```

---

## 4. Estrutura de Pacotes S2S (Protocolo Binario)

A comunicacao de rede entre os nós utiliza pacotes serializados manualmente para velocidade e economia de recursos:

| Campo | Tipo | Descricao |
| :--- | :--- | :--- |
| **Magic Byte** | `u8` | Prefixo identificador fixo `0x53` |
| **Message Type** | `u8` | `1` para Handshake, `2` para PresenceUpdate, `3` para Broadcast, `4` para PrivateMessage, `5` para S2sPing, `6` para S2sPong |
| **Node Origin** | `u8` | ID do nó de origem da acao |
| **Message Seq** | `u32` | Sequencia incremental de mensagens para deduplicacao |
| **Payload Length** | `u32` | Tamanho do payload subsequente em bytes |
| **Payload** | `Vec<u8>` | Bytes de dados do payload |

Qualquer interrupcao ou dados recebidos fora da estrutura esperada resultam no fechamento da conexao imediato por motivos de seguranca.
