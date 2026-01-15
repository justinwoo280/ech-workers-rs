//! UDP 会话管理
//! 
//! 通过 SOCKS5 UDP ASSOCIATE 转发 UDP 流量

use std::collections::HashMap;
use std::net::Ipv4Addr;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};

use super::packet::build_udp_packet;
use super::router::TunWriter;
use super::fake_dns::FakeDnsPool;
use super::socks5_udp::{Socks5UdpSession, Socks5UdpFrame};
use crate::error::Result;

/// UDP 会话超时时间
const UDP_SESSION_TIMEOUT: Duration = Duration::from_secs(60);

/// UDP 会话 Key
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct UdpSessionKey {
    pub local_ip: Ipv4Addr,
    pub local_port: u16,
    pub remote_ip: Ipv4Addr,
    pub remote_port: u16,
}

impl UdpSessionKey {
    pub fn new(local_ip: Ipv4Addr, local_port: u16, remote_ip: Ipv4Addr, remote_port: u16) -> Self {
        Self { local_ip, local_port, remote_ip, remote_port }
    }
}

/// UDP 会话
struct UdpSession {
    /// 创建时间
    created_at: Instant,
    /// 最后活动时间
    last_active: Instant,
    /// 数据发送通道
    tx: mpsc::Sender<Vec<u8>>,
}

impl UdpSession {
    fn is_expired(&self) -> bool {
        self.last_active.elapsed() > UDP_SESSION_TIMEOUT
    }
    
    fn touch(&mut self) {
        self.last_active = Instant::now();
    }
}

/// UDP 会话管理器
pub struct UdpSessionManager {
    /// 会话表
    sessions: Arc<RwLock<HashMap<UdpSessionKey, UdpSession>>>,
    /// SOCKS5 代理地址
    socks5_addr: Option<String>,
    /// FakeDNS 池
    fake_dns: Arc<FakeDnsPool>,
}

impl UdpSessionManager {
    pub fn new(socks5_addr: Option<String>, fake_dns: Arc<FakeDnsPool>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            socks5_addr,
            fake_dns,
        }
    }
    
    /// 处理 UDP 数据包
    pub async fn handle_packet(
        &self,
        src_ip: Ipv4Addr,
        src_port: u16,
        dst_ip: Ipv4Addr,
        dst_port: u16,
        payload: &[u8],
        tun_writer: &TunWriter,
    ) -> Result<()> {
        let key = UdpSessionKey::new(src_ip, src_port, dst_ip, dst_port);
        
        // 检查是否已有会话
        {
            let mut sessions = self.sessions.write().await;
            if let Some(session) = sessions.get_mut(&key) {
                if !session.is_expired() {
                    session.touch();
                    // 发送数据到现有会话
                    let _ = session.tx.send(payload.to_vec()).await;
                    return Ok(());
                } else {
                    // 会话过期，移除
                    sessions.remove(&key);
                }
            }
        }
        
        // 创建新会话
        self.create_session(key, payload, tun_writer.clone()).await
    }
    
    /// 创建新的 UDP 会话
    async fn create_session(
        &self,
        key: UdpSessionKey,
        initial_data: &[u8],
        tun_writer: TunWriter,
    ) -> Result<()> {
        // 解析目标地址（支持 FakeDNS）
        let target_host = if FakeDnsPool::is_fake_ip(key.remote_ip) {
            if let Some(domain) = self.fake_dns.lookup(key.remote_ip) {
                tracing::debug!("UDP FakeDNS resolved: {} -> {}", key.remote_ip, domain);
                domain
            } else {
                key.remote_ip.to_string()
            }
        } else {
            key.remote_ip.to_string()
        };
        
        tracing::info!("Creating UDP session: {}:{} -> {}:{}", 
            key.local_ip, key.local_port, target_host, key.remote_port);
        
        // 创建数据通道
        let (tx, rx) = mpsc::channel::<Vec<u8>>(256);
        
        // 存储会话
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key, UdpSession {
                created_at: Instant::now(),
                last_active: Instant::now(),
                tx: tx.clone(),
            });
        }
        
        // 发送初始数据
        let _ = tx.send(initial_data.to_vec()).await;
        
        // 检查是否配置了 SOCKS5 代理
        let socks5_addr = match &self.socks5_addr {
            Some(addr) => addr.clone(),
            None => {
                tracing::debug!("No SOCKS5 proxy configured, UDP packet dropped");
                return Ok(());
            }
        };
        
        // 启动代理任务
        let sessions = self.sessions.clone();
        let target_host = target_host.to_string();
        
        tokio::spawn(async move {
            if let Err(e) = Self::run_udp_proxy_socks5(
                key,
                &target_host,
                &socks5_addr,
                rx,
                tun_writer,
            ).await {
                tracing::debug!("UDP SOCKS5 proxy error for {:?}: {}", key, e);
            }
            
            // 清理会话
            let mut sessions = sessions.write().await;
            sessions.remove(&key);
        });
        
        Ok(())
    }
    
    /// 运行 UDP 代理 (通过 SOCKS5 UDP ASSOCIATE)
    async fn run_udp_proxy_socks5(
        key: UdpSessionKey,
        target_host: &str,
        socks5_addr: &str,
        mut rx: mpsc::Receiver<Vec<u8>>,
        tun_writer: TunWriter,
    ) -> Result<()> {
        // 建立 SOCKS5 UDP ASSOCIATE 会话
        let session = Socks5UdpSession::connect(socks5_addr).await?;
        
        tracing::info!("SOCKS5 UDP session established: {} -> {}:{}", 
            session.local_addr(), target_host, key.remote_port);
        
        // 判断目标是 IP 还是域名
        let is_ip = target_host.parse::<Ipv4Addr>().is_ok();
        
        // 双向转发
        let mut buf = vec![0u8; 65535];
        
        loop {
            tokio::select! {
                // 从本地收到数据，发送到 SOCKS5 代理
                Some(data) = rx.recv() => {
                    let frame = if is_ip {
                        let ip: Ipv4Addr = target_host.parse().unwrap();
                        Socks5UdpFrame::new_ipv4(ip, key.remote_port, data)
                    } else {
                        Socks5UdpFrame::new_domain(target_host, key.remote_port, data)
                    };
                    
                    if let Err(e) = session.send(&frame).await {
                        tracing::debug!("SOCKS5 UDP send error: {}", e);
                        break;
                    }
                    tracing::trace!("UDP sent {} bytes via SOCKS5", frame.data.len());
                }
                
                // 从 SOCKS5 代理收到数据，写回 TUN
                result = session.recv(&mut buf) => {
                    match result {
                        Ok((frame, _addr)) => {
                            // 构造 UDP 响应包写回 TUN
                            let response_packet = build_udp_packet(
                                key.remote_ip,  // 源 = 远程
                                key.local_ip,   // 目标 = 本地
                                key.remote_port,
                                key.local_port,
                                &frame.data,
                            );
                            let _ = tun_writer.write_packet(response_packet).await;
                            tracing::trace!("UDP received {} bytes via SOCKS5", frame.data.len());
                        }
                        Err(e) => {
                            tracing::debug!("SOCKS5 UDP recv error: {}", e);
                            break;
                        }
                    }
                }
                
                // 超时检查
                _ = tokio::time::sleep(UDP_SESSION_TIMEOUT) => {
                    tracing::debug!("UDP session timeout: {:?}", key);
                    break;
                }
            }
        }
        
        Ok(())
    }
    
    /// 清理过期会话
    pub async fn cleanup_expired(&self) {
        let mut sessions = self.sessions.write().await;
        let expired: Vec<UdpSessionKey> = sessions
            .iter()
            .filter(|(_, s)| s.is_expired())
            .map(|(k, _)| *k)
            .collect();
        
        for key in expired {
            sessions.remove(&key);
            tracing::trace!("UDP session expired: {:?}", key);
        }
    }
    
    /// 获取活跃会话数
    pub async fn active_sessions(&self) -> usize {
        let sessions = self.sessions.read().await;
        sessions.iter().filter(|(_, s)| !s.is_expired()).count()
    }
}
