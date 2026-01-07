/// Transport enum - 传输层抽象
/// 
/// ⚠️ 使用 enum 而不是 trait 的原因：
/// - async fn in trait → object safety 地狱
/// - enum = Rust async 的标准解法
/// - hyper、reqwest、quinn 都用这个模式

use crate::error::Result;
use crate::stream::ConnectionContext;
use crate::transport::yamux::{YamuxTransport, WebSocketTransport};

/// Transport enum
/// 
/// 统一的传输层接口，支持多种传输方式
pub enum Transport {
    /// Yamux 多路复用传输
    Yamux(YamuxTransport),
    /// 简单 WebSocket 传输
    WebSocket(WebSocketTransport),
}

impl Transport {
    /// 建立新连接
    pub async fn dial(&self) -> Result<ConnectionContext> {
        match self {
            Transport::Yamux(t) => t.dial().await,
            Transport::WebSocket(t) => t.dial().await,
        }
    }

    /// 返回传输层名称
    pub fn name(&self) -> &str {
        match self {
            Transport::Yamux(t) => t.name(),
            Transport::WebSocket(t) => t.name(),
        }
    }
}
