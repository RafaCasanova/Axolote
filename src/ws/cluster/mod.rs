/// Modulo Cluster — Comunicacao distribuida entre servidores WebSocket
/// Leader-per-Room com replicacao assincrona e failover automatico.
/// Este modulo e totalmente OPCIONAL. Se nao ativado, o WsHub opera em modo standalone.

pub mod config;
pub mod envelope;
pub mod state;
pub mod peer;
pub mod manager;

pub use self::config::ClusterConfig;
pub use self::state::ClusterState;
pub use self::manager::ClusterManager;
