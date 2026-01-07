/// 全局配置
#[derive(Debug, Clone)]
pub struct Config {
    /// 本地监听地址
    pub listen_addr: String,
    
    /// 服务器地址
    pub server_addr: String,
    
    /// 服务器 IP（可选，用于绕过 DNS）
    pub server_ip: Option<String>,
    
    /// 认证 token
    pub token: String,
    
    /// 是否启用 ECH
    pub use_ech: bool,
    
    /// ECH 查询域名
    pub ech_domain: String,
    
    /// DoH 服务器
    pub doh_server: String,
    
    /// 是否启用 Yamux 多路复用
    pub use_yamux: bool,
    
    /// 是否启用指纹随机化
    pub randomize_fingerprint: bool,
}
