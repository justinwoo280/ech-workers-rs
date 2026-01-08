use clap::{Parser, Subcommand};
use tracing::{info, error};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

mod config;
mod error;
mod transport;
mod proxy;
mod ech;
mod utils;
mod tls;

use config::Config;
use error::Result;

#[derive(Parser, Debug)]
#[command(name = "ech-workers-rs")]
#[command(about = "Rust implementation of ech-workers with ECH + TLS1.3 + Yamux", long_about = None)]
struct Args {
    #[command(subcommand)]
    command: Commands,

    /// Enable verbose logging
    #[arg(short, long, global = true)]
    verbose: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// Test DoH query for ECH config
    TestDoh {
        /// Domain to query
        domain: String,
        
        /// DoH server URL
        #[arg(short, long, default_value = "https://cloudflare-dns.com/dns-query")]
        doh_server: String,
    },
    
    /// Connect to a host with ECH
    Connect {
        /// Target host
        host: String,

        /// Target port
        #[arg(short, long, default_value_t = 443)]
        port: u16,
        
        /// DoH server for ECH config
        #[arg(short, long, default_value = "https://cloudflare-dns.com/dns-query")]
        doh_server: String,
    },
    
    /// Run proxy server
    Proxy {
        /// Local proxy listen address (SOCKS5 + HTTP)
        #[arg(short = 'l', long, default_value = "127.0.0.1:1080")]
        listen: String,

        /// Server address (e.g., example.com:443)
        #[arg(short = 'f', long)]
        server: String,

        /// Server IP (optional, bypass DNS)
        #[arg(long)]
        server_ip: Option<String>,

        /// Authentication token
        #[arg(short = 't', long)]
        token: String,

        /// Enable ECH (Encrypted Client Hello)
        #[arg(long, default_value = "true")]
        ech: bool,

        /// ECH domain for DoH query
        #[arg(long, default_value = "cloudflare-ech.com")]
        ech_domain: String,

        /// DoH server for ECH config lookup
        #[arg(long, default_value = "dns.alidns.com/dns-query")]
        doh_server: String,

        /// Enable Yamux multiplexing
        #[arg(long, default_value = "true")]
        yamux: bool,

        /// Enable fingerprint randomization
        #[arg(long, default_value = "true")]
        randomize_fingerprint: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // 初始化日志
    let log_level = if args.verbose {
        "debug"
    } else {
        "info"
    };
    
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("ech_workers_rs={},tower_http=debug", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    match args.command {
        Commands::TestDoh { domain, doh_server } => {
            info!("Testing DoH query for {}", domain);
            
            match ech::query_ech_config(&domain, &doh_server).await {
                Ok(ech_config) => {
                    info!("✓ Successfully retrieved ECH config");
                    info!("  Size: {} bytes", ech_config.len());
                    info!("  Hex: {}", hex::encode(&ech_config[..ech_config.len().min(32)]));
                }
                Err(e) => {
                    error!("✗ Failed to query ECH config: {}", e);
                    return Err(e);
                }
            }
        }
        
        Commands::Connect { host, port, doh_server } => {
            info!("Connecting to {}:{}", host, port);
            
            // 1. 查询 ECH 配置
            info!("Querying ECH config via {}", doh_server);
            let ech_config = ech::query_ech_config(&host, &doh_server).await?;
            info!("Got ECH config: {} bytes", ech_config.len());
            
            // 2. 建立 TLS 连接
            info!("Establishing TLS connection with ECH...");
            let config = tls::TunnelConfig::new(&host, port)
                .with_ech(ech_config, true);
            
            let tunnel = tls::TlsTunnel::connect(config)?;
            
            // 3. 获取连接信息
            let info = tunnel.info()?;
            info!("✓ Connection successful");
            info!("  Protocol: {}", info.protocol_version);
            info!("  Cipher: {}", info.cipher_suite);
            info!("  ECH Accepted: {}", info.used_ech);
            
            if !info.used_ech {
                error!("⚠ ECH was not accepted by server!");
                return Err(error::Error::Dns("ECH not accepted".into()));
            }
            
            info!("✓ ECH successfully negotiated!");
        }
        
        Commands::Proxy {
            listen,
            server,
            server_ip,
            token,
            ech,
            ech_domain,
            doh_server,
            yamux,
            randomize_fingerprint,
        } => {
            info!("🚀 ech-workers-rs starting...");
            info!("   Listen: {}", listen);
            info!("   Server: {}", server);
            info!("   ECH: {}", ech);
            info!("   Yamux: {}", yamux);
            info!("   Fingerprint Randomization: {}", randomize_fingerprint);

            // 构建配置
            let config = Config {
                listen_addr: listen,
                server_addr: server,
                server_ip,
                token,
                use_ech: ech,
                ech_domain,
                doh_server,
                use_yamux: yamux,
                randomize_fingerprint,
            };

            // 启动代理服务器
            if let Err(e) = proxy::run_server(config).await {
                error!("❌ Server error: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
