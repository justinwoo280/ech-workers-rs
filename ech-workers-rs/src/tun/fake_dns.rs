//! FakeDNS 模块 - Dual Stack (IPv4 + IPv6)
//! 
//! 实现 Clash 风格的 FakeDNS 模式，支持 IPv4 和 IPv6 双栈
//! 
//! 工作原理：
//! 1. DNS 查询时返回假 IP（198.18.0.0/15 段）
//! 2. IPv6 使用 IPv4 Embedded 策略：fc00::c612:xxxx
//! 3. 存储 假IP <-> 域名 的映射（只维护一套 IPv4 映射）
//! 4. TCP/UDP 连接时查找映射，获取真实域名
//! 5. 代理时使用域名而非 IP
//! 
//! 地址池选择：
//! - IPv4: 198.18.0.0/15 (RFC 2544 基准测试保留段)
//! - IPv6: fc00::/96 (Unique Local Address，嵌入 IPv4)

use std::net::{Ipv4Addr, Ipv6Addr};
use std::sync::{Arc, Mutex};
use std::num::NonZeroUsize;
use dashmap::DashMap;
use lru::LruCache;

use super::dns::{DnsHandler, DnsQuery, DnsQueryType};

/// FakeDNS IP 池范围
/// 使用 198.18.0.0/16 (198.18.0.0 - 198.18.255.255)
const FAKE_IP_START: u32 = 0xC6120000; // 198.18.0.0
const FAKE_IP_POOL_SIZE: usize = 65536; // /16 = 65536 个 IP

/// IPv6 前缀 (fc00::/96)
const IPV6_PREFIX: u128 = 0xfc00_0000_0000_0000_0000_0000_0000_0000;

/// DNS TTL（秒）- 设为 1 秒避免客户端缓存导致映射错乱
const DNS_TTL_SECONDS: u32 = 1;

/// HTTPS 记录类型 (Type 65)
const DNS_TYPE_HTTPS: u16 = 65;

/// FakeDNS 管理器 - 高性能并发版本
/// 
/// 使用 DashMap 实现并发读写，LruCache 实现 IP 回收
pub struct FakeDnsPool {
    /// 域名 -> IPv4 映射（用于去重）
    domain_to_ip: DashMap<String, Ipv4Addr>,
    
    /// IPv4 -> 域名 映射（LRU 用于 IP 回收）
    ip_to_domain: Arc<Mutex<LruCache<Ipv4Addr, String>>>,
    
    /// 当前分配游标
    cursor: Arc<Mutex<u32>>,
    
    /// 是否启用 FakeDNS
    enabled: bool,
}

impl FakeDnsPool {
    /// 创建新的 FakeDNS 池
    pub fn new(enabled: bool) -> Self {
        Self {
            domain_to_ip: DashMap::new(),
            ip_to_domain: Arc::new(Mutex::new(LruCache::new(
                NonZeroUsize::new(FAKE_IP_POOL_SIZE).unwrap()
            ))),
            cursor: Arc::new(Mutex::new(0)),
            enabled,
        }
    }
    
    /// 是否启用
    pub fn is_enabled(&self) -> bool {
        self.enabled
    }
    
    /// 检查 IPv4 是否是假 IP
    pub fn is_fake_ip(ip: Ipv4Addr) -> bool {
        let ip_u32 = u32::from(ip);
        ip_u32 >= FAKE_IP_START && ip_u32 < FAKE_IP_START + FAKE_IP_POOL_SIZE as u32
    }
    
    /// 检查 IPv6 是否是假 IP (fc00::/96 embedded IPv4)
    pub fn is_fake_ipv6(ip: Ipv6Addr) -> bool {
        let ip_u128 = u128::from(ip);
        // 检查前 96 位是否匹配 fc00::
        (ip_u128 & 0xffffffff_ffffffff_ffffffff_00000000) == IPV6_PREFIX
    }
    
    /// 从 IPv6 提取嵌入的 IPv4
    pub fn extract_ipv4_from_ipv6(ipv6: Ipv6Addr) -> Option<Ipv4Addr> {
        if !Self::is_fake_ipv6(ipv6) {
            return None;
        }
        // 取 IPv6 的末尾 32 位
        let ip_u128 = u128::from(ipv6);
        let ipv4_u32 = (ip_u128 & 0xffffffff) as u32;
        Some(Ipv4Addr::from(ipv4_u32))
    }
    
    /// 将 IPv4 映射到 IPv6 (fc00::/96 + IPv4)
    pub fn map_v4_to_v6(v4: Ipv4Addr) -> Ipv6Addr {
        let v4_u32 = u32::from(v4) as u128;
        Ipv6Addr::from(IPV6_PREFIX | v4_u32)
    }
    
    /// 为域名分配假 IP（返回 IPv4 和 IPv6）
    pub fn allocate(&self, domain: &str) -> (Ipv4Addr, Ipv6Addr) {
        // 规范化域名（小写，去除尾部点）
        let domain = domain.to_lowercase().trim_end_matches('.').to_string();
        
        // 1. 先检查缓存
        if let Some(ip) = self.domain_to_ip.get(&domain) {
            let v4 = *ip;
            return (v4, Self::map_v4_to_v6(v4));
        }
        
        // 2. 分配新 IP（轮询）
        let new_ip = {
            let mut cursor = self.cursor.lock().unwrap();
            let next_val = *cursor;
            *cursor = (*cursor + 1) % FAKE_IP_POOL_SIZE as u32;
            Ipv4Addr::from(FAKE_IP_START + next_val)
        };
        
        // 3. 处理 LRU 淘汰
        {
            let mut lru = self.ip_to_domain.lock().unwrap();
            if let Some((evicted_ip, evicted_domain)) = lru.push(new_ip, domain.clone()) {
                // 如果挤掉了旧的 IP，从 domain_to_ip 中删除
                self.domain_to_ip.remove(&evicted_domain);
                tracing::trace!("FakeDNS evicted: {} -> {}", evicted_domain, evicted_ip);
            }
        }
        
        // 4. 写入新记录
        self.domain_to_ip.insert(domain.clone(), new_ip);
        
        tracing::debug!("FakeDNS allocated: {} -> {} / {}", 
            domain, new_ip, Self::map_v4_to_v6(new_ip));
        
        (new_ip, Self::map_v4_to_v6(new_ip))
    }
    
    /// 通过假 IPv4 查找域名
    pub fn lookup(&self, ip: Ipv4Addr) -> Option<String> {
        if !Self::is_fake_ip(ip) {
            return None;
        }
        
        let lru = self.ip_to_domain.lock().unwrap();
        lru.peek(&ip).cloned()
    }
    
    /// 通过假 IPv6 查找域名
    pub fn lookup_v6(&self, ip: Ipv6Addr) -> Option<String> {
        let v4 = Self::extract_ipv4_from_ipv6(ip)?;
        self.lookup(v4)
    }
    
    /// 通过域名查找假 IP
    pub fn lookup_by_domain(&self, domain: &str) -> Option<(Ipv4Addr, Ipv6Addr)> {
        let domain = domain.to_lowercase().trim_end_matches('.').to_string();
        self.domain_to_ip.get(&domain).map(|ip| {
            let v4 = *ip;
            (v4, Self::map_v4_to_v6(v4))
        })
    }
    
    /// 处理 DNS 查询，返回假 IP 响应
    /// 
    /// - A 记录：返回 IPv4 假地址
    /// - AAAA 记录：返回 IPv6 假地址 (IPv4 Embedded)
    /// - HTTPS (Type 65)：返回 NXDOMAIN（阻止浏览器使用 ECH）
    pub fn handle_query(&self, query: &DnsQuery) -> Option<Vec<u8>> {
        let qtype_u16 = query.qtype.to_u16();
        
        // HTTPS 记录 (Type 65) - 返回空响应，阻止 ECH
        if qtype_u16 == DNS_TYPE_HTTPS {
            tracing::debug!("FakeDNS: blocking HTTPS record for {}", query.name);
            return Some(Self::build_empty_response(query));
        }
        
        match query.qtype {
            DnsQueryType::A => {
                let (fake_ip, _) = self.allocate(&query.name);
                let response = Self::build_a_response(query, fake_ip);
                tracing::info!("FakeDNS A: {} -> {}", query.name, fake_ip);
                Some(response)
            }
            DnsQueryType::AAAA => {
                let (_, fake_ip6) = self.allocate(&query.name);
                let response = Self::build_aaaa_response(query, fake_ip6);
                tracing::info!("FakeDNS AAAA: {} -> {}", query.name, fake_ip6);
                Some(response)
            }
            _ => {
                // 其他类型返回 NXDOMAIN
                tracing::debug!("FakeDNS: NXDOMAIN for {} (type {:?})", query.name, query.qtype);
                Some(DnsHandler::build_nxdomain(query))
            }
        }
    }
    
    /// 构造 A 记录响应（TTL=1s）
    fn build_a_response(query: &DnsQuery, ip: Ipv4Addr) -> Vec<u8> {
        let mut response = Vec::with_capacity(64);
        
        // Header
        response.extend(&query.id.to_be_bytes());
        response.extend(&[0x81, 0x80]); // QR=1, RD=1, RA=1
        response.extend(&[0x00, 0x01]); // QDCOUNT = 1
        response.extend(&[0x00, 0x01]); // ANCOUNT = 1
        response.extend(&[0x00, 0x00]); // NSCOUNT = 0
        response.extend(&[0x00, 0x00]); // ARCOUNT = 0
        
        // Question section
        for part in query.name.split('.') {
            response.push(part.len() as u8);
            response.extend(part.as_bytes());
        }
        response.push(0);
        response.extend(&query.qtype.to_u16().to_be_bytes());
        response.extend(&query.qclass.to_be_bytes());
        
        // Answer section
        response.extend(&[0xc0, 0x0c]); // Name pointer
        response.extend(&[0x00, 0x01]); // Type A
        response.extend(&[0x00, 0x01]); // Class IN
        response.extend(&DNS_TTL_SECONDS.to_be_bytes()); // TTL = 1s
        response.extend(&[0x00, 0x04]); // RDLENGTH = 4
        response.extend(&ip.octets());
        
        response
    }
    
    /// 构造 AAAA 记录响应（TTL=1s）
    fn build_aaaa_response(query: &DnsQuery, ip: Ipv6Addr) -> Vec<u8> {
        let mut response = Vec::with_capacity(80);
        
        // Header
        response.extend(&query.id.to_be_bytes());
        response.extend(&[0x81, 0x80]); // QR=1, RD=1, RA=1
        response.extend(&[0x00, 0x01]); // QDCOUNT = 1
        response.extend(&[0x00, 0x01]); // ANCOUNT = 1
        response.extend(&[0x00, 0x00]); // NSCOUNT = 0
        response.extend(&[0x00, 0x00]); // ARCOUNT = 0
        
        // Question section
        for part in query.name.split('.') {
            response.push(part.len() as u8);
            response.extend(part.as_bytes());
        }
        response.push(0);
        response.extend(&query.qtype.to_u16().to_be_bytes());
        response.extend(&query.qclass.to_be_bytes());
        
        // Answer section
        response.extend(&[0xc0, 0x0c]); // Name pointer
        response.extend(&[0x00, 0x1c]); // Type AAAA (28)
        response.extend(&[0x00, 0x01]); // Class IN
        response.extend(&DNS_TTL_SECONDS.to_be_bytes()); // TTL = 1s
        response.extend(&[0x00, 0x10]); // RDLENGTH = 16
        response.extend(&ip.octets());
        
        response
    }
    
    /// 构造空响应（NOERROR 但无 Answer）
    fn build_empty_response(query: &DnsQuery) -> Vec<u8> {
        let mut response = Vec::with_capacity(32);
        
        // Header
        response.extend(&query.id.to_be_bytes());
        response.extend(&[0x81, 0x80]); // QR=1, RD=1, RA=1, RCODE=0 (NOERROR)
        response.extend(&[0x00, 0x01]); // QDCOUNT = 1
        response.extend(&[0x00, 0x00]); // ANCOUNT = 0 (无应答)
        response.extend(&[0x00, 0x00]); // NSCOUNT = 0
        response.extend(&[0x00, 0x00]); // ARCOUNT = 0
        
        // Question section
        for part in query.name.split('.') {
            response.push(part.len() as u8);
            response.extend(part.as_bytes());
        }
        response.push(0);
        response.extend(&query.qtype.to_u16().to_be_bytes());
        response.extend(&query.qclass.to_be_bytes());
        
        response
    }
    
    /// 获取统计信息
    pub fn stats(&self) -> FakeDnsStats {
        let lru = self.ip_to_domain.lock().unwrap();
        FakeDnsStats {
            total_entries: lru.len(),
            pool_size: FAKE_IP_POOL_SIZE,
        }
    }
}

/// FakeDNS 统计信息
#[derive(Debug, Clone)]
pub struct FakeDnsStats {
    pub total_entries: usize,
    pub pool_size: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_fake_dns_allocate() {
        let pool = FakeDnsPool::new(true);
        
        let (ip1, ip6_1) = pool.allocate("example.com");
        let (ip2, _) = pool.allocate("example.com");
        let (ip3, _) = pool.allocate("google.com");
        
        // 同一域名应该返回同一 IP
        assert_eq!(ip1, ip2);
        // 不同域名应该返回不同 IP
        assert_ne!(ip1, ip3);
        
        // 应该是假 IP 范围内
        assert!(FakeDnsPool::is_fake_ip(ip1));
        assert!(FakeDnsPool::is_fake_ip(ip3));
        assert!(FakeDnsPool::is_fake_ipv6(ip6_1));
    }
    
    #[test]
    fn test_fake_dns_lookup() {
        let pool = FakeDnsPool::new(true);
        
        let (ip, ip6) = pool.allocate("test.example.com");
        let domain = pool.lookup(ip);
        let domain6 = pool.lookup_v6(ip6);
        
        assert_eq!(domain, Some("test.example.com".to_string()));
        assert_eq!(domain6, Some("test.example.com".to_string()));
    }
    
    #[test]
    fn test_ipv4_ipv6_mapping() {
        let v4 = Ipv4Addr::new(198, 18, 0, 5);
        let v6 = FakeDnsPool::map_v4_to_v6(v4);
        
        // fc00::c612:0005
        assert!(FakeDnsPool::is_fake_ipv6(v6));
        
        let extracted = FakeDnsPool::extract_ipv4_from_ipv6(v6);
        assert_eq!(extracted, Some(v4));
    }
    
    #[test]
    fn test_is_fake_ip() {
        assert!(FakeDnsPool::is_fake_ip(Ipv4Addr::new(198, 18, 0, 1)));
        assert!(FakeDnsPool::is_fake_ip(Ipv4Addr::new(198, 18, 255, 255)));
        assert!(!FakeDnsPool::is_fake_ip(Ipv4Addr::new(198, 19, 0, 0))); // 超出 /16
        assert!(!FakeDnsPool::is_fake_ip(Ipv4Addr::new(192, 168, 1, 1)));
        assert!(!FakeDnsPool::is_fake_ip(Ipv4Addr::new(8, 8, 8, 8)));
    }
    
    #[test]
    fn test_lru_eviction() {
        let pool = FakeDnsPool::new(true);
        
        // 分配第一个
        let (ip1, _) = pool.allocate("first.com");
        
        // 分配很多，直到第一个被淘汰
        for i in 0..FAKE_IP_POOL_SIZE {
            pool.allocate(&format!("domain{}.com", i));
        }
        
        // 第一个应该被淘汰了
        assert!(pool.lookup(ip1).is_none());
    }
}
