/// Configuracao do Cluster WebSocket
/// Define o ID do no, a porta S2S e os enderecos dos peers iniciais.

/// Configuracao necessaria para ativar o modo cluster no servidor.
/// Passada ao metodo `server.enable_cluster(config)`.
#[derive(Clone)]
pub struct ClusterConfig {
    /// ID unico deste no no cluster (0-255).
    /// Cada instancia do servidor DEVE ter um ID diferente.
    pub node_id: u8,

    /// Porta TCP exclusiva para comunicacao inter-servidor (S2S).
    /// Diferente da porta HTTP/WS de clientes.
    pub s2s_port: String,

    /// Lista de enderecos dos outros nos do cluster (Seed Nodes).
    /// Formato: "IP:PORTA_S2S" (ex: "127.0.0.1:9002").
    pub peers: Vec<String>,

    /// Intervalo em segundos para envio de heartbeat S2S entre nos.
    /// Padrao: 2 segundos.
    pub heartbeat_interval_secs: u64,

    /// Numero de heartbeats perdidos antes de declarar um no como morto.
    /// Padrao: 3 (total = heartbeat_interval * missed = 6 segundos).
    pub heartbeat_missed_limit: u64,
    
    /// Segredo compartilhado opcional para assinar envelopes via HMAC-SHA1.
    pub cluster_secret: Option<String>,
}

impl ClusterConfig {
    /// Cria uma configuracao de cluster com valores padrao para heartbeat.
    pub fn new(node_id: u8, s2s_port: &str, peers: Vec<String>) -> Self {
        ClusterConfig {
            node_id,
            s2s_port: s2s_port.to_string(),
            peers,
            heartbeat_interval_secs: 2,
            heartbeat_missed_limit: 3,
            cluster_secret: None,
        }
    }

    /// Configura uma senha compartilhada para comunicação segura S2S.
    pub fn with_secret(mut self, secret: &str) -> Self {
        self.cluster_secret = Some(secret.to_string());
        self
    }
}
