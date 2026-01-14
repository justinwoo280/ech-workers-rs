//! 用户态 TCP/IP 栈辅助
//! 
//! 处理 TCP 连接状态和数据重组

use std::collections::HashMap;
use std::net::SocketAddr;

/// TCP 连接上下文
pub struct TcpConnection {
    /// 源地址
    pub src: SocketAddr,
    /// 目标地址
    pub dst: SocketAddr,
    /// 序列号
    pub seq: u32,
    /// 确认号
    pub ack: u32,
    /// 窗口大小
    pub window: u16,
    /// 发送缓冲区
    pub send_buffer: Vec<u8>,
    /// 接收缓冲区
    pub recv_buffer: Vec<u8>,
    /// 连接状态
    pub state: TcpState,
}

/// TCP 状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpState {
    Closed,
    Listen,
    SynSent,
    SynReceived,
    Established,
    FinWait1,
    FinWait2,
    CloseWait,
    Closing,
    LastAck,
    TimeWait,
}

impl TcpConnection {
    pub fn new(src: SocketAddr, dst: SocketAddr) -> Self {
        Self {
            src,
            dst,
            seq: rand::random(),
            ack: 0,
            window: 65535,
            send_buffer: Vec::new(),
            recv_buffer: Vec::new(),
            state: TcpState::Closed,
        }
    }
}

/// TUN TCP/IP 栈
pub struct TunStack {
    /// TCP 连接表
    connections: HashMap<(SocketAddr, SocketAddr), TcpConnection>,
}

impl TunStack {
    pub fn new() -> Self {
        Self {
            connections: HashMap::new(),
        }
    }
    
    /// 获取或创建 TCP 连接
    pub fn get_or_create_tcp(&mut self, src: SocketAddr, dst: SocketAddr) -> &mut TcpConnection {
        self.connections
            .entry((src, dst))
            .or_insert_with(|| TcpConnection::new(src, dst))
    }
    
    /// 获取 TCP 连接
    pub fn get_tcp(&mut self, src: SocketAddr, dst: SocketAddr) -> Option<&mut TcpConnection> {
        self.connections.get_mut(&(src, dst))
    }
    
    /// 删除 TCP 连接
    pub fn remove_tcp(&mut self, src: SocketAddr, dst: SocketAddr) {
        self.connections.remove(&(src, dst));
    }
    
    /// 获取连接数
    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for TunStack {
    fn default() -> Self {
        Self::new()
    }
}
