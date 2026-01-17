/// 测试 ECH + TLS + WebSocket + Yamux 连接

use std::sync::Arc;
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[path = "../src/config.rs"]
mod config;
#[path = "../src/error.rs"]
mod error;
#[path = "../src/utils/mod.rs"]
mod utils;
#[path = "../src/ech/mod.rs"]
mod ech;
#[path = "../src/tls/mod.rs"]
mod tls;
#[path = "../src/transport/mod.rs"]
mod transport;

use config::Config;
use transport::{YamuxTransport, WebSocketTransport};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let server = std::env::args().nth(1).unwrap_or_else(|| {
        eprintln!("Usage: test_ws_yamux <server:port/path> [token]");
        eprintln!("Example: test_ws_yamux example.com:443/ws mytoken");
        std::process::exit(1);
    });

    let token = std::env::args().nth(2).unwrap_or_else(|| "test-token".to_string());

    info!("=== ECH + WebSocket + Yamux Test ===");
    info!("Server: {}", server);
    info!("Token: {}", token);
    println!();

    // 创建配置
    let config = Arc::new(Config {
        listen_addr: "127.0.0.1:1080".to_string(),
        server_addr: server.clone(),
        server_ip: None,
        token: token.clone(),
        use_ech: true,
        ech_domain: server.split(':').next().unwrap_or("example.com").to_string(),
        doh_server: "223.5.5.5/dns-query".to_string(),
        use_yamux: true,
        randomize_fingerprint: false,
    });

    // 测试 WebSocket 连接（不使用 Yamux）
    info!("Step 1: Testing WebSocket connection...");
    let ws_transport = WebSocketTransport::new(config.clone());
    match ws_transport.dial().await {
        Ok(_ws) => {
            info!("✅ WebSocket connection successful");
        }
        Err(e) => {
            error!("❌ WebSocket connection failed: {}", e);
            return Err(e.into());
        }
    }
    println!();

    // 测试 Yamux 连接
    info!("Step 2: Testing Yamux connection...");
    let yamux_transport = YamuxTransport::new(config.clone());
    match yamux_transport.dial().await {
        Ok(_stream) => {
            info!("✅ Yamux stream opened successfully");
        }
        Err(e) => {
            error!("❌ Yamux connection failed: {}", e);
            return Err(e.into());
        }
    }
    println!();

    info!("✅✅✅ All tests passed!");
    Ok(())
}
