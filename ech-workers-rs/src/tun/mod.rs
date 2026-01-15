//! TUN 模式模块
//! 
//! 支持 Linux (/dev/net/tun) 和 Windows (wintun.dll) 的虚拟网络设备
//! 
//! ## 架构
//! 
//! ```text
//! TUN Device (10.0.0.1) -> IP Router -> ECH Tunnel -> Remote Server
//! ```

mod device;
mod router;
mod nat;
mod stack;
mod packet;
mod route;
mod tcp_session;
mod dns;
mod fake_dns;

pub use device::TunDevice;
pub use router::TunRouter;
pub use nat::NatTable;
pub use route::RouteConfig;
pub use tcp_session::{TcpSessionManager, TcpSession, SessionKey, TcpAction, ReceivedTcpFlags};
pub use dns::DnsHandler;
pub use fake_dns::FakeDnsPool;

use crate::config::Config;
use crate::error::Result;

/// TUN 模式配置
#[derive(Debug, Clone)]
pub struct TunConfig {
    /// TUN 设备名称
    pub name: String,
    /// TUN 设备 IP 地址
    pub address: std::net::Ipv4Addr,
    /// 子网掩码
    pub netmask: std::net::Ipv4Addr,
    /// 网关地址
    pub gateway: std::net::Ipv4Addr,
    /// MTU
    pub mtu: u16,
    /// DNS 服务器
    pub dns: Vec<std::net::Ipv4Addr>,
    /// 代理配置
    pub proxy_config: Config,
    /// 是否启用 FakeDNS
    pub fake_dns: bool,
}

impl Default for TunConfig {
    fn default() -> Self {
        Self {
            name: "tun0".to_string(),
            address: std::net::Ipv4Addr::new(10, 0, 0, 1),
            netmask: std::net::Ipv4Addr::new(255, 255, 255, 0),
            gateway: std::net::Ipv4Addr::new(10, 0, 0, 1),
            mtu: 1500,
            dns: vec![std::net::Ipv4Addr::new(8, 8, 8, 8)],
            proxy_config: Config::default(),
            fake_dns: true, // 默认启用 FakeDNS
        }
    }
}

/// 启动 TUN 模式
pub async fn run_tun(config: TunConfig, server_ip: Option<std::net::Ipv4Addr>) -> Result<()> {
    tracing::info!("🚀 Starting TUN mode...");
    tracing::info!("   Device: {}", config.name);
    tracing::info!("   Address: {}/{}", config.address, config.netmask);
    tracing::info!("   MTU: {}", config.mtu);
    
    // 创建 TUN 设备
    let device = TunDevice::create(&config)?;
    tracing::info!("✅ TUN device created");
    
    // 配置路由表
    let mut route_config = RouteConfig::new(&config.name, config.address, config.gateway);
    if let Some(ip) = server_ip {
        route_config = route_config.with_server_ip(ip);
    }
    route_config.setup()?;
    tracing::info!("✅ Routes configured");
    
    // 创建路由器
    let mut router = TunRouter::new(device, config.clone());
    
    // 运行路由器（路由表会在 route_config drop 时自动清理）
    let result = router.run().await;
    
    // 手动清理路由（确保清理）
    let _ = route_config.cleanup();
    
    result
}
