//! TUN 设备抽象层
//! 
//! 跨平台 TUN 设备创建和管理
//! - Linux: /dev/net/tun (tun crate)
//! - Windows: wintun.dll (tun crate with wintun backend)

use crate::error::{Error, Result};
use super::TunConfig;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// TUN 设备包装器
pub struct TunDevice {
    inner: tun::AsyncDevice,
    name: String,
}

impl TunDevice {
    /// 创建 TUN 设备
    pub fn create(config: &TunConfig) -> Result<Self> {
        let mut tun_config = tun::Configuration::default();
        
        tun_config
            .address(config.address)
            .netmask(config.netmask)
            .destination(config.gateway)
            .mtu(config.mtu as i32)
            .up();
        
        // Linux 特定配置
        #[cfg(target_os = "linux")]
        tun_config.platform_config(|p| {
            p.ensure_root_privileges(true);
        });
        
        // 设置设备名称
        if !config.name.is_empty() {
            tun_config.name(&config.name);
        }
        
        let device = tun::create_as_async(&tun_config)
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to create TUN device: {}", e)
            )))?;
        
        let name = config.name.clone();
        
        tracing::info!("TUN device created: {}", name);
        
        // Windows: 配置 IP 地址 (wintun 需要额外配置)
        #[cfg(target_os = "windows")]
        Self::configure_windows_adapter(config)?;
        
        Ok(Self { inner: device, name })
    }
    
    #[cfg(target_os = "windows")]
    fn configure_windows_adapter(config: &TunConfig) -> Result<()> {
        use std::process::Command;
        
        // 等待适配器就绪
        std::thread::sleep(std::time::Duration::from_millis(500));
        
        // 使用 netsh 配置 IP 地址
        let addr_str = format!("{}", config.address);
        let mask_str = format!("{}", config.netmask);
        
        let output = Command::new("netsh")
            .args([
                "interface", "ip", "set", "address",
                &format!("name={}", config.name),
                "static", &addr_str, &mask_str
            ])
            .output()
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to configure adapter: {}", e)
            )))?;
        
        if !output.status.success() {
            tracing::warn!("netsh set address: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        // 配置 DNS
        if !config.dns.is_empty() {
            let dns_str = format!("{}", config.dns[0]);
            let _ = Command::new("netsh")
                .args([
                    "interface", "ip", "set", "dns",
                    &format!("name={}", config.name),
                    "static", &dns_str
                ])
                .output();
        }
        
        Ok(())
    }
    
    /// 读取 IP 包
    pub async fn read_packet(&mut self, buf: &mut [u8]) -> Result<usize> {
        let n = self.inner.read(buf).await
            .map_err(Error::Io)?;
        Ok(n)
    }
    
    /// 写入 IP 包
    pub async fn write_packet(&mut self, buf: &[u8]) -> Result<usize> {
        let n = self.inner.write(buf).await
            .map_err(Error::Io)?;
        Ok(n)
    }
    
    /// 获取设备名称
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for TunDevice {
    fn drop(&mut self) {
        tracing::info!("Closing TUN device: {}", self.name);
    }
}
