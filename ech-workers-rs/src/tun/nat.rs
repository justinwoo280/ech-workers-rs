//! NAT 表 - 连接追踪
//! 
//! 跟踪 TCP/UDP 连接，将内部地址映射到外部连接

use std::collections::HashMap;
use std::net::{SocketAddr, Ipv4Addr};
use std::time::{Duration, Instant};
use tokio::sync::RwLock;

/// NAT 条目
#[derive(Debug, Clone)]
pub struct NatEntry {
    /// 源地址（内部）
    pub src: SocketAddr,
    /// 目标地址（外部）
    pub dst: SocketAddr,
    /// 协议 (TCP=6, UDP=17)
    pub protocol: u8,
    /// 创建时间
    pub created: Instant,
    /// 最后活动时间
    pub last_active: Instant,
    /// 连接状态
    pub state: ConnectionState,
}

/// 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectionState {
    /// 新建
    New,
    /// SYN 已发送
    SynSent,
    /// 已建立
    Established,
    /// FIN 等待
    FinWait,
    /// 已关闭
    Closed,
}

/// NAT 键
#[derive(Debug, Clone, Copy, Hash, PartialEq, Eq)]
pub struct NatKey {
    pub src_addr: Ipv4Addr,
    pub src_port: u16,
    pub dst_addr: Ipv4Addr,
    pub dst_port: u16,
    pub protocol: u8,
}

impl NatKey {
    pub fn new(src: SocketAddr, dst: SocketAddr, protocol: u8) -> Self {
        let (src_addr, src_port) = match src {
            SocketAddr::V4(v4) => (*v4.ip(), v4.port()),
            SocketAddr::V6(_) => (Ipv4Addr::UNSPECIFIED, 0),
        };
        let (dst_addr, dst_port) = match dst {
            SocketAddr::V4(v4) => (*v4.ip(), v4.port()),
            SocketAddr::V6(_) => (Ipv4Addr::UNSPECIFIED, 0),
        };
        
        Self {
            src_addr,
            src_port,
            dst_addr,
            dst_port,
            protocol,
        }
    }
    
    /// 反向键（用于响应包匹配）
    pub fn reverse(&self) -> Self {
        Self {
            src_addr: self.dst_addr,
            src_port: self.dst_port,
            dst_addr: self.src_addr,
            dst_port: self.src_port,
            protocol: self.protocol,
        }
    }
}

/// NAT 表
pub struct NatTable {
    /// 连接表
    entries: RwLock<HashMap<NatKey, NatEntry>>,
    /// TCP 超时
    tcp_timeout: Duration,
    /// UDP 超时
    udp_timeout: Duration,
    /// 最大条目数
    max_entries: usize,
}

impl NatTable {
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(HashMap::new()),
            tcp_timeout: Duration::from_secs(300),   // 5 分钟
            udp_timeout: Duration::from_secs(60),    // 1 分钟
            max_entries: 65536,
        }
    }
    
    /// 查找或创建 NAT 条目
    pub async fn lookup_or_create(
        &self,
        src: SocketAddr,
        dst: SocketAddr,
        protocol: u8,
    ) -> Option<NatEntry> {
        let key = NatKey::new(src, dst, protocol);
        
        // 先尝试查找
        {
            let entries = self.entries.read().await;
            if let Some(entry) = entries.get(&key) {
                return Some(entry.clone());
            }
        }
        
        // 创建新条目
        let now = Instant::now();
        let entry = NatEntry {
            src,
            dst,
            protocol,
            created: now,
            last_active: now,
            state: ConnectionState::New,
        };
        
        let mut entries = self.entries.write().await;
        
        // 检查容量
        if entries.len() >= self.max_entries {
            // 清理过期条目
            self.cleanup_expired_locked(&mut entries);
        }
        
        entries.insert(key, entry.clone());
        Some(entry)
    }
    
    /// 查找 NAT 条目（用于响应包）
    pub async fn lookup_reverse(
        &self,
        src: SocketAddr,
        dst: SocketAddr,
        protocol: u8,
    ) -> Option<NatEntry> {
        let key = NatKey::new(src, dst, protocol).reverse();
        
        let entries = self.entries.read().await;
        entries.get(&key).cloned()
    }
    
    /// 更新连接状态
    pub async fn update_state(
        &self,
        src: SocketAddr,
        dst: SocketAddr,
        protocol: u8,
        state: ConnectionState,
    ) {
        let key = NatKey::new(src, dst, protocol);
        
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&key) {
            entry.state = state;
            entry.last_active = Instant::now();
        }
    }
    
    /// 更新最后活动时间
    pub async fn touch(
        &self,
        src: SocketAddr,
        dst: SocketAddr,
        protocol: u8,
    ) {
        let key = NatKey::new(src, dst, protocol);
        
        let mut entries = self.entries.write().await;
        if let Some(entry) = entries.get_mut(&key) {
            entry.last_active = Instant::now();
        }
    }
    
    /// 删除条目
    pub async fn remove(
        &self,
        src: SocketAddr,
        dst: SocketAddr,
        protocol: u8,
    ) {
        let key = NatKey::new(src, dst, protocol);
        
        let mut entries = self.entries.write().await;
        entries.remove(&key);
    }
    
    /// 清理过期条目
    pub async fn cleanup_expired(&self) {
        let mut entries = self.entries.write().await;
        self.cleanup_expired_locked(&mut entries);
    }
    
    fn cleanup_expired_locked(&self, entries: &mut HashMap<NatKey, NatEntry>) {
        let now = Instant::now();
        
        entries.retain(|_, entry| {
            let timeout = if entry.protocol == 6 {
                self.tcp_timeout
            } else {
                self.udp_timeout
            };
            
            now.duration_since(entry.last_active) < timeout
        });
    }
    
    /// 获取当前条目数
    pub async fn len(&self) -> usize {
        self.entries.read().await.len()
    }
    
    /// 获取统计信息
    pub async fn stats(&self) -> NatStats {
        let entries = self.entries.read().await;
        
        let mut tcp_count = 0;
        let mut udp_count = 0;
        let mut established = 0;
        
        for entry in entries.values() {
            if entry.protocol == 6 {
                tcp_count += 1;
                if entry.state == ConnectionState::Established {
                    established += 1;
                }
            } else if entry.protocol == 17 {
                udp_count += 1;
            }
        }
        
        NatStats {
            total: entries.len(),
            tcp_count,
            udp_count,
            established,
        }
    }
}

impl Default for NatTable {
    fn default() -> Self {
        Self::new()
    }
}

/// NAT 统计
#[derive(Debug, Clone)]
pub struct NatStats {
    pub total: usize,
    pub tcp_count: usize,
    pub udp_count: usize,
    pub established: usize,
}
