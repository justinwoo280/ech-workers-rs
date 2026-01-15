//! GUI 配置管理

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use anyhow::Result;

/// GUI 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GuiConfig {
    /// 基本设置
    #[serde(default)]
    pub basic: BasicConfig,
    
    /// ECH 设置
    #[serde(default)]
    pub ech: EchConfig,
    
    /// 高级设置
    #[serde(default)]
    pub advanced: AdvancedConfig,
    
    /// 应用设置
    #[serde(default)]
    pub app: AppConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BasicConfig {
    /// 监听地址
    #[serde(default = "default_listen_addr")]
    pub listen_addr: String,
    
    /// 服务器地址
    #[serde(default = "default_server_addr")]
    pub server_addr: String,
    
    /// 认证 Token
    #[serde(default)]
    pub token: String,
    
    /// 启用 TUN 模式
    #[serde(default)]
    pub enable_tun: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchConfig {
    /// 启用 ECH
    #[serde(default = "default_true")]
    pub enabled: bool,
    
    /// ECH 域名
    #[serde(default = "default_ech_domain")]
    pub domain: String,
    
    /// DoH 服务器
    #[serde(default = "default_doh_server")]
    pub doh_server: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdvancedConfig {
    /// 启用 Yamux 多路复用
    #[serde(default = "default_true")]
    pub enable_yamux: bool,
    
    /// 启用指纹随机化
    #[serde(default = "default_true")]
    pub enable_fingerprint_randomization: bool,
    
    /// TLS 指纹配置
    #[serde(default = "default_tls_profile")]
    pub tls_profile: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    /// 开机自启
    #[serde(default)]
    pub auto_start: bool,
    
    /// 启动时最小化
    #[serde(default)]
    pub start_minimized: bool,
    
    /// 最小化到托盘
    #[serde(default = "default_true")]
    pub minimize_to_tray: bool,
    
    /// 关闭时最小化到托盘
    #[serde(default = "default_true")]
    pub close_to_tray: bool,
}

impl Default for GuiConfig {
    fn default() -> Self {
        Self {
            basic: BasicConfig::default(),
            ech: EchConfig::default(),
            advanced: AdvancedConfig::default(),
            app: AppConfig::default(),
        }
    }
}

impl Default for BasicConfig {
    fn default() -> Self {
        Self {
            listen_addr: default_listen_addr(),
            server_addr: default_server_addr(),
            token: String::new(),
            enable_tun: false,
        }
    }
}

impl Default for EchConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            domain: default_ech_domain(),
            doh_server: default_doh_server(),
        }
    }
}

impl Default for AdvancedConfig {
    fn default() -> Self {
        Self {
            enable_yamux: true,
            enable_fingerprint_randomization: true,
            tls_profile: default_tls_profile(),
        }
    }
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            auto_start: false,
            start_minimized: false,
            minimize_to_tray: true,
            close_to_tray: true,
        }
    }
}

// 默认值函数
fn default_listen_addr() -> String {
    "127.0.0.1:1080".to_string()
}

fn default_server_addr() -> String {
    "your-worker.workers.dev".to_string()
}

fn default_ech_domain() -> String {
    "cloudflare-ech.com".to_string()
}

fn default_doh_server() -> String {
    "https://1.1.1.1/dns-query".to_string()
}

fn default_tls_profile() -> String {
    "Chrome".to_string()
}

fn default_true() -> bool {
    true
}

impl GuiConfig {
    /// 获取配置文件路径
    pub fn config_path() -> Result<PathBuf> {
        let config_dir = directories::ProjectDirs::from("com", "ech-workers", "ech-workers-rs")
            .ok_or_else(|| anyhow::anyhow!("无法获取配置目录"))?;
        
        let config_dir = config_dir.config_dir();
        std::fs::create_dir_all(config_dir)?;
        
        Ok(config_dir.join("config.toml"))
    }

    /// 加载配置
    pub fn load() -> Result<Self> {
        let path = Self::config_path()?;
        
        if !path.exists() {
            return Ok(Self::default());
        }
        
        let content = std::fs::read_to_string(&path)?;
        let config: Self = toml::from_str(&content)?;
        
        Ok(config)
    }

    /// 保存配置
    pub fn save(&self) -> Result<()> {
        let path = Self::config_path()?;
        let content = toml::to_string_pretty(self)?;
        std::fs::write(&path, content)?;
        
        Ok(())
    }
}
