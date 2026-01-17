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
mod tun;
mod gui;
mod rpc;

use config::Config;
use error::Result;

#[derive(Parser, Debug)]
#[command(name = "ech-workers-rs")]
#[command(version)]
#[command(author = "ech-workers-rs contributors")]
#[command(about = "支持 ECH (Encrypted Client Hello) 的高性能代理客户端")]
#[command(long_about = r#"
ech-workers-rs - 支持 ECH 的安全代理客户端

功能特性:
  • TLS 1.3 + Encrypted Client Hello (ECH) 加密
  • 模拟 Firefox 浏览器 TLS 指纹
  • 支持 SOCKS5 和 HTTP CONNECT 代理协议
  • Yamux 多路复用提升性能
  • DoH (DNS over HTTPS) 获取 ECH 配置

快速开始:
  启动本地代理服务器:
    ech-workers-rs proxy -f 服务器地址:443 -t 认证密钥

  然后配置浏览器/系统代理为:
    SOCKS5 代理: 127.0.0.1:1080
    HTTP 代理:   127.0.0.1:1080

使用示例:
  # 使用默认设置启动代理
  ech-workers-rs proxy -f myserver.com:443 -t secret123

  # 自定义端口并启用详细日志
  ech-workers-rs proxy -l 0.0.0.0:8080 -f myserver.com:443 -t secret123 -v

  # 测试 ECH 配置获取
  ech-workers-rs test-doh cloudflare.com

  # 测试 ECH 连接
  ech-workers-rs connect cloudflare.com
"#)]
struct Args {
    #[command(subcommand)]
    command: Option<Commands>,

    /// 启用详细日志输出
    #[arg(short, long, global = true)]
    verbose: bool,
    
    /// JSON-RPC 模式 (用于 GUI 通信)
    #[arg(long, global = true)]
    json_rpc: bool,
}

#[derive(Subcommand, Debug)]
enum Commands {
    /// 启动 GUI 界面
    Gui,
    
    /// 测试 DoH 查询获取 ECH 配置
    TestDoh {
        /// 要查询的域名
        domain: String,
        
        /// DoH 服务器地址
        #[arg(short, long, default_value = "https://cloudflare-dns.com/dns-query")]
        doh_server: String,
    },
    
    /// 测试 ECH 连接到指定主机
    Connect {
        /// 目标主机
        host: String,

        /// 目标端口
        #[arg(short, long, default_value_t = 443)]
        port: u16,
        
        /// DoH 服务器地址
        #[arg(short, long, default_value = "https://cloudflare-dns.com/dns-query")]
        doh_server: String,
    },
    
    /// 启动本地代理服务器 (支持 SOCKS5 和 HTTP CONNECT)
    Proxy {
        /// 本地监听地址 (同时支持 SOCKS5 和 HTTP)
        #[arg(short = 'l', long, default_value = "127.0.0.1:1080")]
        listen: String,

        /// 远程服务器地址 (例如: example.com:443)
        #[arg(short = 'f', long)]
        server: String,

        /// 服务器 IP (可选，用于绕过 DNS 解析)
        #[arg(long)]
        server_ip: Option<String>,

        /// 认证密钥/Token
        #[arg(short = 't', long)]
        token: String,

        /// 启用 ECH (Encrypted Client Hello)
        #[arg(long, default_value = "true")]
        ech: bool,

        /// ECH 查询域名
        #[arg(long, default_value = "cloudflare-ech.com")]
        ech_domain: String,

        /// DoH 服务器地址 (用于获取 ECH 配置)
        #[arg(long, default_value = "dns.alidns.com/dns-query")]
        doh_server: String,

        /// 启用 Yamux 多路复用
        #[arg(long, default_value = "true")]
        yamux: bool,

        /// 启用 TLS 指纹随机化
        #[arg(long, default_value = "true")]
        randomize_fingerprint: bool,
    },
    
    /// 启动 TUN 模式 (透明代理，需要管理员权限)
    Tun {
        /// TUN 设备名称
        #[arg(long, default_value = "tun0")]
        name: String,
        
        /// TUN 设备 IP 地址
        #[arg(long, default_value = "10.0.0.1")]
        address: String,
        
        /// 子网掩码
        #[arg(long, default_value = "255.255.255.0")]
        netmask: String,
        
        /// 远程服务器地址 (例如: example.com:443)
        #[arg(short = 'f', long)]
        server: String,
        
        /// 认证密钥/Token
        #[arg(short = 't', long)]
        token: String,
        
        /// 启用 ECH (Encrypted Client Hello)
        #[arg(long, default_value = "true")]
        ech: bool,
        
        /// ECH 查询域名
        #[arg(long, default_value = "cloudflare-ech.com")]
        ech_domain: String,
        
        /// DoH 服务器地址
        #[arg(long, default_value = "dns.alidns.com/dns-query")]
        doh_server: String,
        
        /// DNS 服务器
        #[arg(long, default_value = "8.8.8.8")]
        dns: String,
        
        /// MTU 大小
        #[arg(long, default_value = "1500")]
        mtu: u16,
        
        /// 启用 FakeDNS 模式
        #[arg(long, default_value = "true")]
        fake_dns: bool,
        
        /// 本地 SOCKS5 代理地址 (用于 UDP ASSOCIATE)
        #[arg(long, default_value = "127.0.0.1:1080")]
        socks5: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();

    // JSON-RPC 模式优先处理
    if args.json_rpc {
        return rpc::RpcServer::run().await;
    }

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

    let command = match args.command {
        Some(cmd) => cmd,
        None => {
            // 无子命令时显示帮助信息
            println!();
            println!("ech-workers-rs - 支持 ECH 的安全代理客户端");
            println!();
            println!("用法: ech-workers-rs <命令> [选项]");
            println!();
            println!("命令:");
            println!("  proxy      启动本地代理服务器 (支持 SOCKS5 和 HTTP CONNECT)");
            println!("  connect    测试 ECH 连接到指定主机");
            println!("  test-doh   测试 DoH 查询获取 ECH 配置");
            println!("  help       显示帮助信息");
            println!();
            println!("快速开始:");
            println!("  ech-workers-rs proxy -f 服务器地址:443 -t 认证密钥");
            println!();
            println!("示例:");
            println!("  # 启动代理 (默认监听 127.0.0.1:1080)");
            println!("  ech-workers-rs proxy -f myserver.com:443 -t secret123");
            println!();
            println!("  # 自定义监听地址并启用详细日志");
            println!("  ech-workers-rs proxy -l 0.0.0.0:8080 -f myserver.com:443 -t secret123 -v");
            println!();
            println!("  # 测试 ECH 配置获取");
            println!("  ech-workers-rs test-doh cloudflare.com");
            println!();
            println!("  # 测试 ECH 连接");
            println!("  ech-workers-rs connect cloudflare.com");
            println!();
            println!("代理参数说明:");
            println!("  -l, --listen <地址>     本地监听地址 [默认: 127.0.0.1:1080]");
            println!("  -f, --server <地址>     远程服务器地址 (必填)");
            println!("  -t, --token <密钥>      认证密钥 (必填)");
            println!("      --server-ip <IP>    服务器 IP (可选，绕过 DNS)");
            println!("      --ech <bool>        启用 ECH [默认: true]");
            println!("      --yamux <bool>      启用 Yamux 多路复用 [默认: true]");
            println!("  -v, --verbose           启用详细日志");
            println!();
            println!("更多信息请运行: ech-workers-rs --help");
            return Ok(());
        }
    };

    match command {
        Commands::Gui => {
            info!("Starting GUI...");
            println!("[DEBUG] Before eframe::run_native");
            
            let options = eframe::NativeOptions {
                viewport: egui::ViewportBuilder::default()
                    .with_inner_size([1024.0, 768.0])
                    .with_min_inner_size([800.0, 600.0])
                    .with_title("ECH Workers RS"),
                ..Default::default()
            };
            
            println!("[DEBUG] NativeOptions created");
            
            let result = eframe::run_native(
                "ECH Workers RS",
                options,
                Box::new(|cc| {
                    println!("[DEBUG] Creating EchWorkersApp...");
                    Ok(Box::new(gui::EchWorkersApp::new(cc)))
                }),
            );
            
            println!("[DEBUG] eframe::run_native returned");
            
            if let Err(e) = result {
                error!("GUI error: {}", e);
                println!("[ERROR] GUI error: {}", e);
                return Err(error::Error::Io(std::io::Error::new(std::io::ErrorKind::Other, e.to_string())));
            }
            
            println!("[DEBUG] GUI exited normally");
            return Ok(());
        }
        
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
        
        Commands::Tun {
            name,
            address,
            netmask,
            server,
            token,
            ech,
            ech_domain,
            doh_server,
            dns,
            mtu,
            fake_dns,
            socks5,
        } => {
            info!("🚀 ech-workers-rs TUN mode starting...");
            info!("   Device: {}", name);
            info!("   Address: {}/{}", address, netmask);
            info!("   Server: {}", server);
            info!("   ECH: {}", ech);
            
            // 解析 IP 地址
            let address: std::net::Ipv4Addr = address.parse()
                .map_err(|_| error::Error::Protocol("Invalid TUN address".into()))?;
            let netmask: std::net::Ipv4Addr = netmask.parse()
                .map_err(|_| error::Error::Protocol("Invalid TUN netmask".into()))?;
            let dns_addr: std::net::Ipv4Addr = dns.parse()
                .map_err(|_| error::Error::Protocol("Invalid DNS address".into()))?;
            
            // 构建代理配置
            let proxy_config = Config {
                listen_addr: "0.0.0.0:0".to_string(), // TUN 模式不需要监听
                server_addr: server,
                server_ip: None,
                token,
                use_ech: ech,
                ech_domain,
                doh_server,
                use_yamux: true,
                randomize_fingerprint: true,
            };
            
            // 构建 TUN 配置
            let tun_config = tun::TunConfig {
                name,
                address,
                netmask,
                gateway: address,
                mtu,
                dns: vec![dns_addr],
                proxy_config,
                fake_dns,
                socks5_addr: Some(socks5.clone()),
            };
            
            info!("   FakeDNS: {}", fake_dns);
            info!("   SOCKS5: {}", socks5);
            
            // 解析服务器 IP（用于排除路由）
            let server_ip: Option<std::net::Ipv4Addr> = {
                // 尝试从服务器地址解析 IP
                let server_host = tun_config.proxy_config.server_addr
                    .split(':').next().unwrap_or("");
                server_host.parse().ok()
            };
            
            // 启动 TUN 模式
            if let Err(e) = tun::run_tun(tun_config, server_ip).await {
                error!("❌ TUN error: {}", e);
                return Err(e);
            }
        }
    }

    Ok(())
}
