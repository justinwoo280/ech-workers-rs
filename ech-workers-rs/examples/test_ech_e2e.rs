/// 端到端 ECH 测试
/// 
/// 测试 DoH 查询 + Zig TLS tunnel + ECH 握手

use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

// 直接引用模块
#[path = "../src/error.rs"]
mod error;

#[path = "../src/ech/doh.rs"]
mod doh;

#[path = "../src/tls/ffi.rs"]
mod ffi;

#[path = "../src/tls/tunnel.rs"]
mod tunnel;

use error::Result;
use tunnel::{TlsTunnel, TunnelConfig};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let host = std::env::args().nth(1).unwrap_or_else(|| "crypto.cloudflare.com".to_string());
    let doh_server = std::env::args().nth(2).unwrap_or_else(|| "https://cloudflare-dns.com/dns-query".to_string());

    info!("=== ECH End-to-End Test ===");
    info!("Host: {}", host);
    info!("DoH Server: {}", doh_server);
    println!();

    // 1. 查询 ECH 配置
    info!("Step 1: Querying ECH config...");
    let ech_config = doh::query_ech_config(&host, &doh_server).await?;
    info!("✅ Got ECH config: {} bytes", ech_config.len());
    println!();

    // 2. 建立 TLS 连接
    info!("Step 2: Establishing TLS connection with ECH...");
    let config = TunnelConfig::new(&host, 443)
        .with_ech(ech_config, true);  // enforce_ech = true
    
    let tunnel = TlsTunnel::connect(config)?;
    info!("✅ TLS connection established");
    println!();

    // 3. 验证 ECH
    info!("Step 3: Verifying ECH status...");
    let info = tunnel.info()?;
    info!("Protocol: {}", info.protocol_version);
    info!("Cipher: {}", info.cipher_suite);
    info!("ECH Accepted: {}", info.used_ech);
    println!();

    if info.used_ech {
        info!("✅✅✅ SUCCESS: ECH was accepted by server!");
        info!("End-to-end ECH integration is working correctly!");
    } else {
        error!("❌ FAILED: ECH was not accepted by server");
        return Err(error::Error::Dns("ECH not accepted".into()));
    }

    Ok(())
}
