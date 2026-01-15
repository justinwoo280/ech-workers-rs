//! UDP 会话管理
//! 
//! 实现 UDP over TCP，通过 Yamux 隧道转发 UDP 流量

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::{mpsc, RwLock};
use futures::{AsyncReadExt, AsyncWriteExt};

use super::packet::build_udp_packet;
use super::router::TunWriter;
use super::fake_dns::FakeDnsPool;
use crate::error::{Error, Result};
use crate::transport::YamuxTransport;
use crate::config::Config;

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
    /// 代理配置
    config: Config,
    /// FakeDNS 池
    fake_dns: Arc<FakeDnsPool>,
}

impl UdpSessionManager {
    pub fn new(config: Config, fake_dns: Arc<FakeDnsPool>) -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            config,
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
            if let Some(domain) = self.fake_dns.lookup(key.remote_ip).await {
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
        
        // 启动代理任务
        let config = self.config.clone();
        let sessions = self.sessions.clone();
        
        tokio::spawn(async move {
            if let Err(e) = Self::run_udp_proxy(
                key,
                &target_host,
                config,
                rx,
                tun_writer,
            ).await {
                tracing::debug!("UDP proxy error for {:?}: {}", key, e);
            }
            
            // 清理会话
            let mut sessions = sessions.write().await;
            sessions.remove(&key);
        });
        
        Ok(())
    }
    
    /// 运行 UDP 代理
    async fn run_udp_proxy(
        key: UdpSessionKey,
        target_host: &str,
        config: Config,
        mut rx: mpsc::Receiver<Vec<u8>>,
        tun_writer: TunWriter,
    ) -> Result<()> {
        // 建立 Yamux 连接
        let config_arc = Arc::new(config);
        let transport = YamuxTransport::new(config_arc);
        let mut stream = transport.dial().await?;
        
        // 发送 UDP 代理请求头
        // 格式: "UDP:<host>:<port>\n"
        let header = format!("UDP:{}:{}\n", target_host, key.remote_port);
        stream.write_all(header.as_bytes()).await?;
        
        tracing::debug!("UDP proxy established: {}:{}", target_host, key.remote_port);
        
        // 双向转发
        let mut buf = vec![0u8; 65535];
        
        loop {
            tokio::select! {
                // 从本地收到数据，发送到远程
                Some(data) = rx.recv() => {
                    // 封装 UDP 数据帧
                    // 格式: [长度 2 字节][数据]
                    let len = data.len() as u16;
                    stream.write_all(&len.to_be_bytes()).await?;
                    stream.write_all(&data).await?;
                    tracing::trace!("UDP sent {} bytes to remote", data.len());
                }
                
                // 从远程收到数据，写回 TUN
                result = stream.read(&mut buf[..2]) => {
                    match result {
                        Ok(0) => {
                            tracing::debug!("UDP remote closed");
                            break;
                        }
                        Ok(2) => {
                            // 读取长度
                            let len = u16::from_be_bytes([buf[0], buf[1]]) as usize;
                            if len > buf.len() {
                                tracing::warn!("UDP packet too large: {}", len);
                                break;
                            }
                            
                            // 读取数据
                            let mut read = 0;
                            while read < len {
                                match stream.read(&mut buf[read..len]).await {
                                    Ok(0) => break,
                                    Ok(n) => read += n,
                                    Err(e) => {
                                        tracing::debug!("UDP read error: {}", e);
                                        break;
                                    }
                                }
                            }
                            
                            if read == len {
                                // 构造 UDP 响应包写回 TUN
                                let response_packet = build_udp_packet(
                                    key.remote_ip,  // 源 = 远程
                                    key.local_ip,   // 目标 = 本地
                                    key.remote_port,
                                    key.local_port,
                                    &buf[..len],
                                );
                                let _ = tun_writer.write_packet(response_packet).await;
                                tracing::trace!("UDP received {} bytes from remote", len);
                            }
                        }
                        Ok(_) => {
                            tracing::debug!("UDP incomplete length read");
                            break;
                        }
                        Err(e) => {
                            tracing::debug!("UDP read error: {}", e);
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
