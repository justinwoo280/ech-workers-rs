//! 路由表配置
//! 
//! 自动配置系统路由表，将流量导向 TUN 设备

use crate::error::{Error, Result};
use std::net::Ipv4Addr;
use std::process::Command;

/// 路由配置
pub struct RouteConfig {
    /// TUN 设备名称
    pub device_name: String,
    /// TUN 设备 IP
    pub device_ip: Ipv4Addr,
    /// 网关 IP
    pub gateway: Ipv4Addr,
    /// 服务器 IP（需要排除，走原始路由）
    pub server_ip: Option<Ipv4Addr>,
    /// 原始默认网关（用于恢复）
    pub original_gateway: Option<Ipv4Addr>,
}

impl RouteConfig {
    pub fn new(device_name: &str, device_ip: Ipv4Addr, gateway: Ipv4Addr) -> Self {
        Self {
            device_name: device_name.to_string(),
            device_ip,
            gateway,
            server_ip: None,
            original_gateway: None,
        }
    }
    
    /// 设置服务器 IP（排除路由）
    pub fn with_server_ip(mut self, ip: Ipv4Addr) -> Self {
        self.server_ip = Some(ip);
        self
    }
    
    /// 配置路由表
    pub fn setup(&mut self) -> Result<()> {
        tracing::info!("Setting up routes for TUN device: {}", self.device_name);
        
        #[cfg(target_os = "windows")]
        self.setup_windows()?;
        
        #[cfg(target_os = "linux")]
        self.setup_linux()?;
        
        Ok(())
    }
    
    /// 清理路由表（恢复原始状态）
    pub fn cleanup(&self) -> Result<()> {
        tracing::info!("Cleaning up routes for TUN device: {}", self.device_name);
        
        #[cfg(target_os = "windows")]
        self.cleanup_windows()?;
        
        #[cfg(target_os = "linux")]
        self.cleanup_linux()?;
        
        Ok(())
    }
    
    // ==================== Windows ====================
    
    #[cfg(target_os = "windows")]
    fn setup_windows(&mut self) -> Result<()> {
        // 获取原始默认网关
        self.original_gateway = Self::get_default_gateway_windows();
        
        if let Some(original_gw) = self.original_gateway {
            tracing::info!("Original default gateway: {}", original_gw);
            
            // 如果指定了服务器 IP，添加直连路由（绕过 TUN）
            if let Some(server_ip) = self.server_ip {
                tracing::info!("Adding direct route for server: {}", server_ip);
                let _ = Command::new("route")
                    .args(["add", &server_ip.to_string(), "mask", "255.255.255.255", &original_gw.to_string()])
                    .output();
            }
        }
        
        // 添加默认路由通过 TUN
        // 使用两个更具体的路由来覆盖默认路由
        tracing::info!("Adding default route via TUN gateway: {}", self.gateway);
        
        // 0.0.0.0/1 -> TUN
        let output = Command::new("route")
            .args(["add", "0.0.0.0", "mask", "128.0.0.0", &self.gateway.to_string()])
            .output()
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to add route: {}", e)
            )))?;
        
        if !output.status.success() {
            tracing::warn!("route add 0.0.0.0/1: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        // 128.0.0.0/1 -> TUN
        let output = Command::new("route")
            .args(["add", "128.0.0.0", "mask", "128.0.0.0", &self.gateway.to_string()])
            .output()
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to add route: {}", e)
            )))?;
        
        if !output.status.success() {
            tracing::warn!("route add 128.0.0.0/1: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        tracing::info!("Routes configured successfully");
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    fn cleanup_windows(&self) -> Result<()> {
        // 删除 TUN 路由
        let _ = Command::new("route")
            .args(["delete", "0.0.0.0", "mask", "128.0.0.0"])
            .output();
        
        let _ = Command::new("route")
            .args(["delete", "128.0.0.0", "mask", "128.0.0.0"])
            .output();
        
        // 删除服务器直连路由
        if let Some(server_ip) = self.server_ip {
            let _ = Command::new("route")
                .args(["delete", &server_ip.to_string()])
                .output();
        }
        
        tracing::info!("Routes cleaned up");
        Ok(())
    }
    
    #[cfg(target_os = "windows")]
    fn get_default_gateway_windows() -> Option<Ipv4Addr> {
        // 通过 PowerShell 获取默认网关
        let output = Command::new("powershell")
            .args(["-Command", "(Get-NetRoute -DestinationPrefix '0.0.0.0/0' | Select-Object -First 1).NextHop"])
            .output()
            .ok()?;
        
        let gateway_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
        gateway_str.parse().ok()
    }
    
    // ==================== Linux ====================
    
    #[cfg(target_os = "linux")]
    fn setup_linux(&mut self) -> Result<()> {
        // 获取原始默认网关
        self.original_gateway = Self::get_default_gateway_linux();
        
        if let Some(original_gw) = self.original_gateway {
            tracing::info!("Original default gateway: {}", original_gw);
            
            // 如果指定了服务器 IP，添加直连路由
            if let Some(server_ip) = self.server_ip {
                tracing::info!("Adding direct route for server: {}", server_ip);
                let _ = Command::new("ip")
                    .args(["route", "add", &format!("{}/32", server_ip), "via", &original_gw.to_string()])
                    .output();
            }
        }
        
        // 添加默认路由通过 TUN
        tracing::info!("Adding default route via TUN: {}", self.device_name);
        
        // 0.0.0.0/1 -> TUN
        let output = Command::new("ip")
            .args(["route", "add", "0.0.0.0/1", "dev", &self.device_name])
            .output()
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to add route: {}", e)
            )))?;
        
        if !output.status.success() {
            tracing::warn!("ip route add 0.0.0.0/1: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        // 128.0.0.0/1 -> TUN
        let output = Command::new("ip")
            .args(["route", "add", "128.0.0.0/1", "dev", &self.device_name])
            .output()
            .map_err(|e| Error::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("Failed to add route: {}", e)
            )))?;
        
        if !output.status.success() {
            tracing::warn!("ip route add 128.0.0.0/1: {}", String::from_utf8_lossy(&output.stderr));
        }
        
        tracing::info!("Routes configured successfully");
        Ok(())
    }
    
    #[cfg(target_os = "linux")]
    fn cleanup_linux(&self) -> Result<()> {
        // 删除 TUN 路由
        let _ = Command::new("ip")
            .args(["route", "del", "0.0.0.0/1", "dev", &self.device_name])
            .output();
        
        let _ = Command::new("ip")
            .args(["route", "del", "128.0.0.0/1", "dev", &self.device_name])
            .output();
        
        // 删除服务器直连路由
        if let Some(server_ip) = self.server_ip {
            let _ = Command::new("ip")
                .args(["route", "del", &format!("{}/32", server_ip)])
                .output();
        }
        
        tracing::info!("Routes cleaned up");
        Ok(())
    }
    
    #[cfg(target_os = "linux")]
    fn get_default_gateway_linux() -> Option<Ipv4Addr> {
        let output = Command::new("ip")
            .args(["route", "show", "default"])
            .output()
            .ok()?;
        
        let route_str = String::from_utf8_lossy(&output.stdout);
        // 格式: "default via 192.168.1.1 dev eth0 ..."
        for part in route_str.split_whitespace() {
            if let Ok(ip) = part.parse::<Ipv4Addr>() {
                return Some(ip);
            }
        }
        None
    }
}

impl Drop for RouteConfig {
    fn drop(&mut self) {
        if let Err(e) = self.cleanup() {
            tracing::error!("Failed to cleanup routes: {}", e);
        }
    }
}
