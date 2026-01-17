/// Transport 层 - L6
/// 
/// 这一层负责建立具体的连接（WebSocket + Yamux）
/// TLS 握手由 Zig 模块处理 (zig-tls-tunnel)

pub mod websocket;
pub mod yamux_optimized;
pub mod connection;

// 导出优化版本
pub use yamux_optimized::YamuxTransport;
