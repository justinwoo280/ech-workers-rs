/// Test DoH query functionality

use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[path = "../src/error.rs"]
mod error;

#[path = "../src/ech/doh.rs"]
mod doh;

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

    let domain = std::env::args().nth(1).unwrap_or_else(|| "cloudflare.com".to_string());
    let doh_server = std::env::args().nth(2).unwrap_or_else(|| "223.5.5.5/dns-query".to_string());

    info!("Testing DoH query for {}", domain);
    info!("Using DoH server: {}", doh_server);

    match doh::query_ech_config(&domain, &doh_server).await {
        Ok(ech_config) => {
            info!("✓ Successfully retrieved ECH config");
            info!("  Size: {} bytes", ech_config.len());
            info!("  Hex (first 32 bytes): {}", hex::encode(&ech_config[..ech_config.len().min(32)]));
            Ok(())
        }
        Err(e) => {
            error!("✗ Failed to query ECH config: {}", e);
            Err(e.into())
        }
    }
}
