//! TCP 会话管理
//! 
//! 实现完整的 TCP 状态机，处理三次握手和数据传输

use std::collections::HashMap;
use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};

use super::packet::{build_tcp_packet, TcpFlags};
use super::router::TunWriter;
use crate::error::{Error, Result};

/// TCP 连接状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    /// 等待 SYN
    Listen,
    /// 收到 SYN，已发送 SYN+ACK
    SynReceived,
    /// 连接已建立
    Established,
    /// 收到 FIN
    FinWait1,
    /// 等待最后 ACK
    FinWait2,
    /// 关闭等待
    CloseWait,
    /// 最后确认
    LastAck,
    /// 时间等待
    TimeWait,
    /// 已关闭
    Closed,
}

/// TCP 会话信息
#[derive(Debug, Clone)]
pub struct TcpSession {
    /// 连接状态
    pub state: TcpState,
    /// 本地 IP
    pub local_ip: Ipv4Addr,
    /// 本地端口
    pub local_port: u16,
    /// 远程 IP
    pub remote_ip: Ipv4Addr,
    /// 远程端口
    pub remote_port: u16,
    /// 本地序列号（我们发送的）
    pub local_seq: u32,
    /// 本地确认号（我们期望收到的）
    pub local_ack: u32,
    /// 远程序列号（对方发送的，即我们收到的 SYN 的 seq）
    pub remote_seq: u32,
    /// 窗口大小
    pub window_size: u16,
    /// 创建时间
    pub created_at: std::time::Instant,
    /// 最后活动时间
    pub last_activity: std::time::Instant,
}

impl TcpSession {
    /// 创建新会话（收到 SYN 时）
    pub fn new_from_syn(
        local_ip: Ipv4Addr,
        local_port: u16,
        remote_ip: Ipv4Addr,
        remote_port: u16,
        remote_seq: u32,
    ) -> Self {
        let now = std::time::Instant::now();
        // 生成随机初始序列号
        let local_seq: u32 = rand::random();
        
        Self {
            state: TcpState::Listen,
            local_ip,
            local_port,
            remote_ip,
            remote_port,
            local_seq,
            local_ack: remote_seq.wrapping_add(1), // ACK = 对方 SEQ + 1
            remote_seq,
            window_size: 65535,
            created_at: now,
            last_activity: now,
        }
    }
    
    /// 获取连接 key
    pub fn key(&self) -> SessionKey {
        SessionKey {
            local_ip: self.local_ip,
            local_port: self.local_port,
            remote_ip: self.remote_ip,
            remote_port: self.remote_port,
        }
    }
    
    /// 更新活动时间
    pub fn touch(&mut self) {
        self.last_activity = std::time::Instant::now();
    }
    
    /// 检查是否超时
    pub fn is_expired(&self, timeout_secs: u64) -> bool {
        self.last_activity.elapsed().as_secs() > timeout_secs
    }
}

/// 会话键（用于 HashMap）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct SessionKey {
    pub local_ip: Ipv4Addr,
    pub local_port: u16,
    pub remote_ip: Ipv4Addr,
    pub remote_port: u16,
}

impl SessionKey {
    pub fn new(local_ip: Ipv4Addr, local_port: u16, remote_ip: Ipv4Addr, remote_port: u16) -> Self {
        Self { local_ip, local_port, remote_ip, remote_port }
    }
    
    /// 从 SocketAddr 对创建
    pub fn from_addrs(local: SocketAddr, remote: SocketAddr) -> Option<Self> {
        match (local, remote) {
            (SocketAddr::V4(l), SocketAddr::V4(r)) => Some(Self {
                local_ip: *l.ip(),
                local_port: l.port(),
                remote_ip: *r.ip(),
                remote_port: r.port(),
            }),
            _ => None,
        }
    }
}

/// TCP 会话管理器
pub struct TcpSessionManager {
    /// 会话表
    sessions: Arc<RwLock<HashMap<SessionKey, TcpSession>>>,
    /// 数据发送通道（key -> sender）
    data_channels: Arc<RwLock<HashMap<SessionKey, mpsc::Sender<Vec<u8>>>>>,
}

impl TcpSessionManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(RwLock::new(HashMap::new())),
            data_channels: Arc::new(RwLock::new(HashMap::new())),
        }
    }
    
    /// 处理收到的 TCP 包
    pub async fn handle_packet(
        &self,
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
        src_port: u16,
        dst_port: u16,
        seq: u32,
        _ack: u32,
        flags: ReceivedTcpFlags,
        payload: &[u8],
        tun_writer: &TunWriter,
    ) -> Result<TcpAction> {
        // 注意：从 TUN 收到的包，src 是客户端（本地应用），dst 是目标服务器
        // 所以我们的会话 key 是：local=src（客户端），remote=dst（服务器）
        let key = SessionKey::new(src_ip, src_port, dst_ip, dst_port);
        
        // SYN 包 - 新连接
        if flags.syn && !flags.ack {
            return self.handle_syn(key, src_ip, src_port, dst_ip, dst_port, seq, tun_writer).await;
        }
        
        // 查找现有会话
        let mut sessions = self.sessions.write().await;
        let session = match sessions.get_mut(&key) {
            Some(s) => s,
            None => {
                // 没有会话，发送 RST
                tracing::debug!("No session for {:?}, sending RST", key);
                let rst_packet = build_tcp_packet(
                    dst_ip, src_ip, dst_port, src_port,
                    0, seq.wrapping_add(1),
                    TcpFlags::rst(),
                    0, &[],
                );
                let _ = tun_writer.write_packet(rst_packet).await;
                return Ok(TcpAction::None);
            }
        };
        
        session.touch();
        
        match session.state {
            TcpState::SynReceived => {
                // 等待 ACK 完成三次握手
                if flags.ack && !flags.syn {
                    tracing::info!("TCP handshake complete: {}:{} -> {}:{}",
                        src_ip, src_port, dst_ip, dst_port);
                    session.state = TcpState::Established;
                    return Ok(TcpAction::ConnectionEstablished(key));
                }
            }
            
            TcpState::Established => {
                // FIN 包
                if flags.fin {
                    tracing::debug!("Received FIN from client");
                    session.state = TcpState::CloseWait;
                    session.local_ack = seq.wrapping_add(1);
                    
                    // 发送 ACK
                    let ack_packet = build_tcp_packet(
                        dst_ip, src_ip, dst_port, src_port,
                        session.local_seq, session.local_ack,
                        TcpFlags::ack(),
                        session.window_size, &[],
                    );
                    let _ = tun_writer.write_packet(ack_packet).await;
                    
                    return Ok(TcpAction::ConnectionClosing(key));
                }
                
                // RST 包
                if flags.rst {
                    tracing::debug!("Received RST from client");
                    session.state = TcpState::Closed;
                    return Ok(TcpAction::ConnectionReset(key));
                }
                
                // 数据包
                if !payload.is_empty() {
                    // 更新确认号
                    session.local_ack = seq.wrapping_add(payload.len() as u32);
                    
                    // 发送 ACK
                    let ack_packet = build_tcp_packet(
                        dst_ip, src_ip, dst_port, src_port,
                        session.local_seq, session.local_ack,
                        TcpFlags::ack(),
                        session.window_size, &[],
                    );
                    let _ = tun_writer.write_packet(ack_packet).await;
                    
                    // 转发数据到代理
                    return Ok(TcpAction::DataReceived(key, payload.to_vec()));
                }
            }
            
            TcpState::CloseWait => {
                // 等待应用关闭
                if flags.ack {
                    // 可以发送 FIN
                    session.state = TcpState::LastAck;
                    let fin_packet = build_tcp_packet(
                        dst_ip, src_ip, dst_port, src_port,
                        session.local_seq, session.local_ack,
                        TcpFlags::fin_ack(),
                        session.window_size, &[],
                    );
                    session.local_seq = session.local_seq.wrapping_add(1);
                    let _ = tun_writer.write_packet(fin_packet).await;
                }
            }
            
            TcpState::LastAck => {
                if flags.ack {
                    session.state = TcpState::Closed;
                    return Ok(TcpAction::ConnectionClosed(key));
                }
            }
            
            _ => {}
        }
        
        Ok(TcpAction::None)
    }
    
    /// 处理 SYN 包
    async fn handle_syn(
        &self,
        key: SessionKey,
        src_ip: Ipv4Addr,
        src_port: u16,
        dst_ip: Ipv4Addr,
        dst_port: u16,
        seq: u32,
        tun_writer: &TunWriter,
    ) -> Result<TcpAction> {
        tracing::info!("New TCP SYN: {}:{} -> {}:{}", src_ip, src_port, dst_ip, dst_port);
        
        // 创建新会话
        let mut session = TcpSession::new_from_syn(src_ip, src_port, dst_ip, dst_port, seq);
        
        // 发送 SYN+ACK
        let syn_ack_packet = build_tcp_packet(
            dst_ip,              // 源 IP（服务器）
            src_ip,              // 目标 IP（客户端）
            dst_port,            // 源端口
            src_port,            // 目标端口
            session.local_seq,   // 我们的序列号
            session.local_ack,   // ACK = 对方 SEQ + 1
            TcpFlags::syn_ack(),
            session.window_size,
            &[],
        );
        
        // 更新序列号（SYN 消耗一个序列号）
        session.local_seq = session.local_seq.wrapping_add(1);
        session.state = TcpState::SynReceived;
        
        // 存储会话
        {
            let mut sessions = self.sessions.write().await;
            sessions.insert(key, session);
        }
        
        // 发送 SYN+ACK
        tun_writer.write_packet(syn_ack_packet).await?;
        
        tracing::debug!("Sent SYN+ACK for {}:{} -> {}:{}", src_ip, src_port, dst_ip, dst_port);
        
        Ok(TcpAction::SynAckSent(key))
    }
    
    /// 发送数据到客户端（从远程服务器收到的响应）
    pub async fn send_data(
        &self,
        key: &SessionKey,
        data: &[u8],
        tun_writer: &TunWriter,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(key)
            .ok_or_else(|| Error::Protocol("Session not found".into()))?;
        
        if session.state != TcpState::Established {
            return Err(Error::Protocol("Session not established".into()));
        }
        
        session.touch();
        
        // 构造数据包
        let packet = build_tcp_packet(
            session.remote_ip,   // 源（服务器）
            session.local_ip,    // 目标（客户端）
            session.remote_port,
            session.local_port,
            session.local_seq,
            session.local_ack,
            TcpFlags::psh_ack(),
            session.window_size,
            data,
        );
        
        // 更新序列号
        session.local_seq = session.local_seq.wrapping_add(data.len() as u32);
        
        tun_writer.write_packet(packet).await
    }
    
    /// 关闭连接（发送 FIN）
    pub async fn close_connection(
        &self,
        key: &SessionKey,
        tun_writer: &TunWriter,
    ) -> Result<()> {
        let mut sessions = self.sessions.write().await;
        let session = sessions.get_mut(key)
            .ok_or_else(|| Error::Protocol("Session not found".into()))?;
        
        if session.state == TcpState::Established {
            // 发送 FIN
            let fin_packet = build_tcp_packet(
                session.remote_ip,
                session.local_ip,
                session.remote_port,
                session.local_port,
                session.local_seq,
                session.local_ack,
                TcpFlags::fin_ack(),
                session.window_size,
                &[],
            );
            
            session.local_seq = session.local_seq.wrapping_add(1);
            session.state = TcpState::FinWait1;
            
            tun_writer.write_packet(fin_packet).await?;
        }
        
        Ok(())
    }
    
    /// 移除会话
    pub async fn remove_session(&self, key: &SessionKey) {
        let mut sessions = self.sessions.write().await;
        sessions.remove(key);
        
        let mut channels = self.data_channels.write().await;
        channels.remove(key);
    }
    
    /// 注册数据通道
    pub async fn register_data_channel(&self, key: SessionKey, tx: mpsc::Sender<Vec<u8>>) {
        let mut channels = self.data_channels.write().await;
        channels.insert(key, tx);
    }
    
    /// 获取数据通道
    pub async fn get_data_channel(&self, key: &SessionKey) -> Option<mpsc::Sender<Vec<u8>>> {
        let channels = self.data_channels.read().await;
        channels.get(key).cloned()
    }
    
    /// 清理过期会话
    pub async fn cleanup_expired(&self, timeout_secs: u64) {
        let mut sessions = self.sessions.write().await;
        let mut channels = self.data_channels.write().await;
        
        let expired: Vec<SessionKey> = sessions.iter()
            .filter(|(_, s)| s.is_expired(timeout_secs))
            .map(|(k, _)| *k)
            .collect();
        
        for key in expired {
            tracing::debug!("Removing expired session: {:?}", key);
            sessions.remove(&key);
            channels.remove(&key);
        }
    }
}

/// 收到的 TCP 标志
#[derive(Debug, Clone, Copy, Default)]
pub struct ReceivedTcpFlags {
    pub syn: bool,
    pub ack: bool,
    pub fin: bool,
    pub rst: bool,
    pub psh: bool,
}

/// TCP 动作（handle_packet 的返回值）
#[derive(Debug)]
pub enum TcpAction {
    /// 无操作
    None,
    /// 已发送 SYN+ACK
    SynAckSent(SessionKey),
    /// 连接已建立（三次握手完成）
    ConnectionEstablished(SessionKey),
    /// 收到数据
    DataReceived(SessionKey, Vec<u8>),
    /// 连接正在关闭
    ConnectionClosing(SessionKey),
    /// 连接已关闭
    ConnectionClosed(SessionKey),
    /// 连接被重置
    ConnectionReset(SessionKey),
}
