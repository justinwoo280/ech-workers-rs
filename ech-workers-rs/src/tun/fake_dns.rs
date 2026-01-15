//! FakeDNS 模块
//! 
//! 实现 Clash 风格的 FakeDNS 模式
//! 
//! 工作原理：
//! 1. DNS 查询时返回假 IP（198.18.0.0/15 段）
//! 2. 存储 假IP <-> 域名 的映射
//! 3. TCP 连接时查找映射，获取真实域名
//! 4. 代理时使用域名而非 IP

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

use super::dns::{DnsHandler, DnsQuery, DnsQueryType};
use crate::error::{Error, Result};

/// FakeDNS IP 池范围
/// 使用 198.18.0.0/15 (198.18.0.0 - 198.19.255.255)
const FAKE_IP_START: u32 = 0xC6120000; // 198.18.0.0
const FAKE_IP_END: u32 = 0xC613FFFF;   // 198.19.255.255

/// 默认 TTL（秒）
const DEFAULT_TTL: u64 = 600; // 10 分钟

/// FakeDNS 条目
#[derive(Debug, Clone)]
struct FakeDnsEntry {
    /// 域名
    domain: String,
    /// 假 IP
    fake_ip: Ipv4Addr,
    /// 创建时间
    created_at: Instant,
    /// TTL
    ttl: Duration,
}

impl FakeDnsEntry {
    fn is_expired(&self) -> bool {
        self.created_at.elapsed() > self.ttl
    }
}

/// FakeDNS 管理器
pub struct FakeDnsPool {
    /// 域名 -> 假IP 映射
    domain_to_ip: Arc<RwLock<HashMap<String, FakeDnsEntry>>>,
    /// 假IP -> 域名 映射（反向查找）
    ip_to_domain: Arc<RwLock<HashMap<Ipv4Addr, String>>>,
    /// 下一个可用的假 IP
    next_ip: Arc<RwLock<u32>>,
    /// 是否启用 FakeDNS
    enabled: bool,
    /// TTL
    ttl: Duration,
}

impl FakeDnsPool {
    /// 创建新的 FakeDNS 池
    pub fn new(enabled: bool) -> Self {
        Self {
            domain_to_ip: Arc::new(RwLock::new(HashMap::new())),
            ip_to_domain: Arc::new(RwLock::new(HashMap::new())),
            next_ip: Arc::new(RwLock::new(FAKE_IP_START)),
            enabled,
            ttl: Duration::from_secs(DEFAULT_TTL),
        }
    }
    
    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// 检查 IP 是否是假 IP
    pub fn is_fake_ip(ip: Ipv4Addr) -> bool {
        let ip_u32 = u32::from(ip);
        ip_u32 >= FAKE_IP_START && ip_u32 <= FAKE_IP_END
    }
    
    /// 为域名分配假 IP
    pub async fn allocate(&self, domain: &str) -> Ipv4Addr {
        // 规范化域名（小写，去除尾部点）
        let domain = domain.to_lowercase().trim_end_matches('.').to_string();
        
        // 检查是否已有映射
        {
            let map = self.domain_to_ip.read().await;
            if let Some(entry) = map.get(&domain) {
                if !entry.is_expired() {
                    return entry.fake_ip;
                }
            }
        }
        
        // 分配新的假 IP
        let fake_ip = {
            let mut next = self.next_ip.write().await;
            let ip = Ipv4Addr::from(*next);
            
            // 循环使用 IP 池
            *next = if *next >= FAKE_IP_END {
                FAKE_IP_START
            } else {
                *next + 1
            };
            
            ip
        };
        
        // 存储映射
        let entry = FakeDnsEntry {
            domain: domain.clone(),
            fake_ip,
            created_at: Instant::now(),
            ttl: self.ttl,
        };
        
        {
            let mut domain_map = self.domain_to_ip.write().await;
            let mut ip_map = self.ip_to_domain.write().await;
            
            // 如果这个 IP 之前被其他域名使用，先清理
            if let Some(old_domain) = ip_map.get(&fake_ip) {
                domain_map.remove(old_domain);
            }
            
            domain_map.insert(domain.clone(), entry);
            ip_map.insert(fake_ip, domain);
        }
        
        tracing::debug!("FakeDNS: {} -> {}", domain, fake_ip);
        fake_ip
    }
    
    /// 通过假 IP 查找域名
    pub async fn lookup(&self, ip: Ipv4Addr) -> Option<String> {
        if !Self::is_fake_ip(ip) {
            return None;
        }
        
        let map = self.ip_to_domain.read().await;
        map.get(&ip).cloned()
    }
    
    /// 通过域名查找假 IP
    pub async fn lookup_by_domain(&self, domain: &str) -> Option<Ipv4Addr> {
        let domain = domain.to_lowercase().trim_end_matches('.').to_string();
        let map = self.domain_to_ip.read().await;
        map.get(&domain).map(|e| e.fake_ip)
    }
    
    /// 处理 DNS 查询，返回假 IP 响应
    pub async fn handle_query(&self, query: &DnsQuery) -> Option<Vec<u8>> {
        // 只处理 A 记录查询
        if query.qtype != DnsQueryType::A {
            return None;
        }
        
        // 分配假 IP
        let fake_ip = self.allocate(&query.name).await;
        
        // 构造 DNS 响应
        let response = DnsHandler::build_response(query, fake_ip);
        
        tracing::info!("FakeDNS response: {} -> {}", query.name, fake_ip);
        Some(response)
    }
    
    /// 清理过期条目
    pub async fn cleanup_expired(&self) {
        let mut domain_map = self.domain_to_ip.write().await;
        let mut ip_map = self.ip_to_domain.write().await;
        
        let expired: Vec<String> = domain_map
            .iter()
            .filter(|(_, entry)| entry.is_expired())
            .map(|(domain, _)| domain.clone())
            .collect();
        
        for domain in expired {
            if let Some(entry) = domain_map.remove(&domain) {
                ip_map.remove(&entry.fake_ip);
                tracing::trace!("FakeDNS expired: {} -> {}", domain, entry.fake_ip);
            }
        }
    }
    
    /// 获取统计信息
    pub async fn stats(&self) -> FakeDnsStats {
        let domain_map = self.domain_to_ip.read().await;
        FakeDnsStats {
            total_entries: domain_map.len(),
            active_entries: domain_map.iter().filter(|(_, e)| !e.is_expired()).count(),
        }
    }
}

/// FakeDNS 统计信息
#[derive(Debug, Clone)]
pub struct FakeDnsStats {
    pub total_entries: usize,
    pub active_entries: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_fake_dns_allocate() {
        let pool = FakeDnsPool::new(true);
        
        let ip1 = pool.allocate("example.com").await;
        let ip2 = pool.allocate("example.com").await;
        let ip3 = pool.allocate("google.com").await;
        
        // 同一域名应该返回同一 IP
        assert_eq!(ip1, ip2);
        // 不同域名应该返回不同 IP
        assert_ne!(ip1, ip3);
        
        // 应该是假 IP 范围内
        assert!(FakeDnsPool::is_fake_ip(ip1));
        assert!(FakeDnsPool::is_fake_ip(ip3));
    }
    
    #[tokio::test]
    async fn test_fake_dns_lookup() {
        let pool = FakeDnsPool::new(true);
        
        let ip = pool.allocate("test.example.com").await;
        let domain = pool.lookup(ip).await;
        
        assert_eq!(domain, Some("test.example.com".to_string()));
    }
    
    #[test]
    fn test_is_fake_ip() {
        assert!(FakeDnsPool::is_fake_ip(Ipv4Addr::new(198, 18, 0, 1)));
        assert!(FakeDnsPool::is_fake_ip(Ipv4Addr::new(198, 19, 255, 255)));
        assert!(!FakeDnsPool::is_fake_ip(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!FakeDnsPool::is_fake_ip(Ipv4Addr::new(8, 8, 8, 8)));
    }
}
