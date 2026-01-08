/// Proxy 层 - L7
/// 
/// 这一层完全类型盲态，只处理代理协议逻辑

pub mod handler;
pub mod socks5;
pub mod http;

use tokio::net::TcpListener;
use tracing::{info, error};
use std::sync::Arc;

use crate::config::Config;
use crate::error::Result;
use crate::transport::{Transport, YamuxTransport, WebSocketTransport};

/// 运行代理服务器
pub async fn run_server(config: Config) -> Result<()> {
    let config = Arc::new(config);
    
    // 创建传输层（使用 enum）
    let transport = if config.use_yamux {
        Transport::Yamux(YamuxTransport::new(config.clone()))
    } else {
        Transport::WebSocket(WebSocketTransport::new(config.clone()))
    };
    let transport = Arc::new(transport);

    info!("🚀 Starting proxy server on {}", config.listen_addr);
    info!("   Transport: {}", transport.name());

    // 监听本地端口
    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("✅ Listening on {}", config.listen_addr);

    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                let transport = transport.clone();
                tokio::spawn(async move {
                    info!("📥 New connection from {}", addr);
                    if let Err(e) = handler::handle_connection(socket, transport).await {
                        error!("❌ Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("❌ Accept error: {}", e);
            }
        }
    }
}
