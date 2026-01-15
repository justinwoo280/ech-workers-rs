/// ECH (Encrypted Client Hello) 配置查询
/// 
/// 通过 DNS-over-HTTPS 查询 HTTPS 记录获取 ECH 配置

pub mod doh;
pub mod config;

pub use doh::query_ech_config;
