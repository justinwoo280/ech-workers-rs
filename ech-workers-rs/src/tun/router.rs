//! TUN 路由器
//! 
//! 处理 IP 包路由，将 TCP/UDP 流量通过 ECH 隧道转发

use super::{TunConfig, TunDevice, NatTable};
use super::nat::ConnectionState;
use super::packet::{build_tcp_packet, build_udp_packet, TcpFlags};
use super::tcp_session::{TcpSessionManager, SessionKey, TcpAction, ReceivedTcpFlags};
use crate::error::{Error, Result};
use crate::transport::YamuxTransport;
use crate::config::Config;

use std::collections::HashMap;
use std::net::{SocketAddr, SocketAddrV4, Ipv4Addr};
use std::sync::Arc;
use tokio::sync::{mpsc, RwLock};
use futures::{AsyncReadExt, AsyncWriteExt};
use etherparse::SlicedPacket;

/// 协议号
const PROTO_TCP: u8 = 6;
const PROTO_UDP: u8 = 17;

/// TUN 路由器
pub struct TunRouter {
    /// TUN 设备
    device: TunDevice,
    /// 配置
    config: TunConfig,
    /// NAT 表
    nat_table: Arc<NatTable>,
    /// TCP 会话管理器
    tcp_sessions: Arc<TcpSessionManager>,
    /// TCP 连接映射 (本地端口 -> 远程流)
    tcp_connections: Arc<RwLock<HashMap<u16, TcpProxyConnection>>>,
    /// UDP 连接映射
    udp_connections: Arc<RwLock<HashMap<u16, UdpProxyConnection>>>,
    /// 下一个本地端口
    next_local_port: Arc<std::sync::atomic::AtomicU16>,
}

/// TCP 代理连接
struct TcpProxyConnection {
    src: SocketAddr,
    dst: SocketAddr,
    /// 发送数据到远程
    tx: mpsc::Sender<Vec<u8>>,
}

/// UDP 代理连接
struct UdpProxyConnection {
    src: SocketAddr,
    dst: SocketAddr,
    tx: mpsc::Sender<Vec<u8>>,
}

/// TUN 写入器（用于将响应包写回 TUN 设备）
#[derive(Clone)]
pub struct TunWriter {
    tx: mpsc::Sender<Vec<u8>>,
}

impl TunWriter {
    /// 写入 IP 包到 TUN 设备
    pub async fn write_packet(&self, packet: Vec<u8>) -> Result<()> {
        self.tx.send(packet).await
            .map_err(|_| Error::Io(std::io::Error::new(
                std::io::ErrorKind::BrokenPipe,
                "TUN writer channel closed"
            )))
    }
}

impl TunRouter {
    pub fn new(device: TunDevice, config: TunConfig) -> Self {
        Self {
            device,
            config,
            nat_table: Arc::new(NatTable::new()),
            tcp_sessions: Arc::new(TcpSessionManager::new()),
            tcp_connections: Arc::new(RwLock::new(HashMap::new())),
            udp_connections: Arc::new(RwLock::new(HashMap::new())),
            next_local_port: Arc::new(std::sync::atomic::AtomicU16::new(10000)),
        }
    }
    
    /// 运行路由器
    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("TUN router starting...");
        
        // 创建 TUN 写入通道
        let (tun_writer_tx, mut tun_writer_rx) = mpsc::channel::<Vec<u8>>(4096);
        let tun_writer = TunWriter { tx: tun_writer_tx };
        
        // 启动 NAT 清理任务
        let nat_table = self.nat_table.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                nat_table.cleanup_expired().await;
            }
        });
        
        // 主循环：读取 TUN 设备上的 IP 包，同时处理写入请求
        let mut buf = vec![0u8; 65535];
        
        loop {
            tokio::select! {
                // 从 TUN 设备读取包
                result = self.device.read_packet(&mut buf) => {
                    match result {
                        Ok(0) => {
                            tracing::warn!("TUN device closed");
                            break;
                        }
                        Ok(n) => {
                            let packet = buf[..n].to_vec();
                            
                            // 在新任务中处理包
                            let config = self.config.clone();
                            let nat_table = self.nat_table.clone();
                            let tcp_conns = self.tcp_connections.clone();
                            let udp_conns = self.udp_connections.clone();
                            let next_port = self.next_local_port.clone();
                            let writer = tun_writer.clone();
                            
                            let tcp_sessions = self.tcp_sessions.clone();
                            
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_packet(
                                    packet,
                                    config,
                                    nat_table,
                                    tcp_sessions,
                                    tcp_conns,
                                    udp_conns,
                                    next_port,
                                    writer,
                                ).await {
                                    tracing::debug!("Packet handling error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            tracing::error!("TUN read error: {}", e);
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                        }
                    }
                }
                
                // 写入包到 TUN 设备
                Some(packet) = tun_writer_rx.recv() => {
                    if let Err(e) = self.device.write_packet(&packet).await {
                        tracing::debug!("TUN write error: {}", e);
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 处理单个 IP 包
    async fn handle_packet(
        packet: Vec<u8>,
        config: TunConfig,
        nat_table: Arc<NatTable>,
        tcp_sessions: Arc<TcpSessionManager>,
        tcp_conns: Arc<RwLock<HashMap<u16, TcpProxyConnection>>>,
        udp_conns: Arc<RwLock<HashMap<u16, UdpProxyConnection>>>,
        next_port: Arc<std::sync::atomic::AtomicU16>,
        tun_writer: TunWriter,
    ) -> Result<()> {
        // 解析 IP 包
        let parsed = SlicedPacket::from_ip(&packet)
            .map_err(|e| Error::Protocol(format!("Invalid IP packet: {:?}", e)))?;
        
        // 获取 IP 头信息
        let (src_ip, dst_ip, protocol) = match &parsed.net {
            Some(etherparse::NetSlice::Ipv4(ipv4)) => {
                let header = ipv4.header();
                let src = Ipv4Addr::from(header.source());
                let dst = Ipv4Addr::from(header.destination());
                let proto = header.protocol().0;
                (src, dst, proto)
            }
            _ => {
                // 只处理 IPv4
                return Ok(());
            }
        };
        
        // 根据协议处理
        match protocol {
            PROTO_TCP => {
                Self::handle_tcp_packet(
                    &packet,
                    &parsed,
                    src_ip,
                    dst_ip,
                    config,
                    nat_table,
                    tcp_sessions,
                    tcp_conns,
                    next_port,
                    tun_writer,
                ).await?;
            }
            PROTO_UDP => {
                Self::handle_udp_packet(
                    &packet,
                    &parsed,
                    src_ip,
                    dst_ip,
                    config,
                    nat_table,
                    udp_conns,
                    next_port,
                    tun_writer,
                ).await?;
            }
            _ => {
                tracing::trace!("Unsupported protocol: {}", protocol);
            }
        }
        
        Ok(())
    }
    
    /// 处理 TCP 包
    async fn handle_tcp_packet(
        _packet: &[u8],
        parsed: &SlicedPacket<'_>,
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
        config: TunConfig,
        nat_table: Arc<NatTable>,
        tcp_sessions: Arc<TcpSessionManager>,
        _tcp_conns: Arc<RwLock<HashMap<u16, TcpProxyConnection>>>,
        _next_port: Arc<std::sync::atomic::AtomicU16>,
        tun_writer: TunWriter,
    ) -> Result<()> {
        // 解析 TCP 头
        let tcp = match &parsed.transport {
            Some(etherparse::TransportSlice::Tcp(tcp)) => tcp,
            _ => return Ok(()),
        };
        
        let src_port = tcp.source_port();
        let dst_port = tcp.destination_port();
        let seq = tcp.sequence_number();
        let ack = tcp.acknowledgment_number();
        
        // 构造标志位
        let flags = ReceivedTcpFlags {
            syn: tcp.syn(),
            ack: tcp.ack(),
            fin: tcp.fin(),
            rst: tcp.rst(),
            psh: tcp.psh(),
        };
        
        // 获取 payload
        let payload = tcp.payload();
        
        tracing::debug!("TCP {}:{} -> {}:{} (SYN={}, ACK={}, FIN={}, RST={}, seq={}, ack={}, len={})", 
            src_ip, src_port, dst_ip, dst_port, 
            flags.syn, flags.ack, flags.fin, flags.rst,
            seq, ack, payload.len());
        
        // 使用会话管理器处理
        let action = tcp_sessions.handle_packet(
            src_ip, dst_ip, src_port, dst_port,
            seq, ack, flags, payload, &tun_writer,
        ).await?;
        
        // 根据动作执行相应操作
        match action {
            TcpAction::ConnectionEstablished(key) => {
                tracing::info!("TCP connection established: {:?}", key);
                
                // 创建 NAT 条目
                let src = SocketAddr::V4(SocketAddrV4::new(src_ip, src_port));
                let dst = SocketAddr::V4(SocketAddrV4::new(dst_ip, dst_port));
                nat_table.lookup_or_create(src, dst, PROTO_TCP).await;
                nat_table.update_state(src, dst, PROTO_TCP, ConnectionState::Established).await;
                
                // 启动代理任务
                let (tx, rx) = mpsc::channel::<Vec<u8>>(1024);
                tcp_sessions.register_data_channel(key, tx).await;
                
                let proxy_config = config.proxy_config.clone();
                let sessions = tcp_sessions.clone();
                let writer = tun_writer.clone();
                let nat = nat_table.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = Self::run_tcp_proxy_v2(
                        key,
                        proxy_config,
                        sessions,
                        nat,
                        rx,
                        writer,
                    ).await {
                        tracing::error!("TCP proxy error for {:?}: {}", key, e);
                    }
                });
            }
            
            TcpAction::DataReceived(key, data) => {
                // 转发数据到代理
                if let Some(tx) = tcp_sessions.get_data_channel(&key).await {
                    let _ = tx.send(data).await;
                }
            }
            
            TcpAction::ConnectionClosing(key) | TcpAction::ConnectionClosed(key) | TcpAction::ConnectionReset(key) => {
                tracing::debug!("TCP connection closing/closed: {:?}", key);
                let src = SocketAddr::V4(SocketAddrV4::new(key.local_ip, key.local_port));
                let dst = SocketAddr::V4(SocketAddrV4::new(key.remote_ip, key.remote_port));
                nat_table.update_state(src, dst, PROTO_TCP, ConnectionState::Closed).await;
            }
            
            _ => {}
        }
        
        Ok(())
    }
    
    /// 运行 TCP 代理（v2 - 使用会话管理器）
    async fn run_tcp_proxy_v2(
        key: SessionKey,
        config: Config,
        tcp_sessions: Arc<TcpSessionManager>,
        nat_table: Arc<NatTable>,
        mut rx: mpsc::Receiver<Vec<u8>>,
        tun_writer: TunWriter,
    ) -> Result<()> {
        tracing::debug!("Starting TCP proxy v2: {:?}", key);
        
        // 建立到远程服务器的连接
        let config_arc = Arc::new(config);
        let transport = YamuxTransport::new(config_arc);
        
        let mut stream = transport.dial().await?;
        
        // 发送目标地址
        let target = format!("{}:{}\n", key.remote_ip, key.remote_port);
        stream.write_all(target.as_bytes()).await?;
        
        tracing::info!("TCP proxy v2 established: {:?}", key);
        
        // 双向转发
        let mut buf = vec![0u8; 32768];
        
        loop {
            tokio::select! {
                // 从 TUN 收到数据，发送到远程
                Some(data) = rx.recv() => {
                    if let Err(e) = stream.write_all(&data).await {
                        tracing::debug!("Write to remote failed: {}", e);
                        break;
                    }
                }
                
                // 从远程收到数据，通过会话管理器写回 TUN
                result = stream.read(&mut buf) => {
                    match result {
                        Ok(0) => {
                            tracing::debug!("Remote closed connection");
                            break;
                        }
                        Ok(n) => {
                            // 通过会话管理器发送数据（会正确处理序列号）
                            if let Err(e) = tcp_sessions.send_data(&key, &buf[..n], &tun_writer).await {
                                tracing::debug!("Send data via session failed: {}", e);
                                break;
                            }
                            tracing::trace!("Sent {} bytes response to TUN via session", n);
                        }
                        Err(e) => {
                            tracing::debug!("Read from remote failed: {}", e);
                            break;
                        }
                    }
                }
            }
        }
        
        // 关闭连接
        let _ = tcp_sessions.close_connection(&key, &tun_writer).await;
        tcp_sessions.remove_session(&key).await;
        
        // 更新 NAT 状态
        let src = SocketAddr::V4(SocketAddrV4::new(key.local_ip, key.local_port));
        let dst = SocketAddr::V4(SocketAddrV4::new(key.remote_ip, key.remote_port));
        nat_table.update_state(src, dst, PROTO_TCP, ConnectionState::Closed).await;
        
        Ok(())
    }
    
    /// 处理 UDP 包
    async fn handle_udp_packet(
        _packet: &[u8],
        parsed: &SlicedPacket<'_>,
        src_ip: Ipv4Addr,
        dst_ip: Ipv4Addr,
        _config: TunConfig,
        nat_table: Arc<NatTable>,
        _udp_conns: Arc<RwLock<HashMap<u16, UdpProxyConnection>>>,
        _next_port: Arc<std::sync::atomic::AtomicU16>,
        _tun_writer: TunWriter,
    ) -> Result<()> {
        // 解析 UDP 头
        let udp = match &parsed.transport {
            Some(etherparse::TransportSlice::Udp(udp)) => udp,
            _ => return Ok(()),
        };
        
        let src_port = udp.source_port();
        let dst_port = udp.destination_port();
        let src = SocketAddr::V4(SocketAddrV4::new(src_ip, src_port));
        let dst = SocketAddr::V4(SocketAddrV4::new(dst_ip, dst_port));
        
        let payload_len = if let Some(etherparse::TransportSlice::Udp(udp_slice)) = &parsed.transport {
            udp_slice.payload().len()
        } else {
            0
        };
        tracing::debug!("UDP {} -> {} ({} bytes)", src, dst, payload_len);
        
        // DNS 查询特殊处理 (端口 53)
        if dst_port == 53 {
            tracing::debug!("DNS query intercepted");
            // TODO: 通过 DoH 处理 DNS
        }
        
        // 创建或更新 NAT 条目
        nat_table.lookup_or_create(src, dst, PROTO_UDP).await;
        
        // UDP 代理逻辑...
        // TODO: 实现 UDP 代理
        
        Ok(())
    }
}
