/// Transport 层 - L6
/// 
/// 这一层负责建立具体的连接（TLS + WebSocket + Yamux）

pub mod tls;
pub mod websocket;
pub mod yamux;
pub mod yamux_optimized;
pub mod connection;

// 导出优化版本
pub use yamux_optimized::{YamuxTransport, WebSocketTransport};
