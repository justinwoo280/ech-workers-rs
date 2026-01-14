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

pub use device::TunDevice;
pub use router::TunRouter;
pub use nat::NatTable;

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
        }
    }
}

/// 启动 TUN 模式
pub async fn run_tun(config: TunConfig) -> Result<()> {
    tracing::info!("🚀 Starting TUN mode...");
    tracing::info!("   Device: {}", config.name);
    tracing::info!("   Address: {}/{}", config.address, config.netmask);
    tracing::info!("   MTU: {}", config.mtu);
    
    // 创建 TUN 设备
    let device = TunDevice::create(&config)?;
    tracing::info!("✅ TUN device created");
    
    // 创建路由器
    let mut router = TunRouter::new(device, config.clone());
    
    // 运行路由器
    router.run().await
}
