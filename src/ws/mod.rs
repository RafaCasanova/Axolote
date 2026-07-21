/// Módulo WebSocket — Implementação nativa sobre TCP (RFC 6455)
/// Sem dependências externas. SHA-1 e Base64 implementados na mão.

pub mod crypto;
pub mod frame;
pub mod handshake;
pub mod connection;
pub mod hub;
pub mod security;
pub mod cluster; // NEW

pub use self::connection::{WsConnection, WsMode, WsMessage, WsRouteConfig};
pub use self::hub::WsHub;
pub use self::cluster::{ClusterConfig, ClusterState, ClusterManager}; // NEW

/// Assinatura padrão para os handlers WebSocket
/// O handler recebe a conexão por valor, além de uma referência ao Hub compartilhado
pub type WsHandlerFn = fn(WsConnection, WsHub);
