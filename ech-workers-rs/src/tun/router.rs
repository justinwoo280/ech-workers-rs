//! TUN 路由器
//! 
//! 处理 IP 包路由，将 TCP/UDP 流量通过 ECH 隧道转发

use super::{TunConfig, TunDevice, NatTable};
use super::nat::ConnectionState;
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
    tx: mpsc::Sender<Vec<u8>>,
}

/// UDP 代理连接
struct UdpProxyConnection {
    src: SocketAddr,
    dst: SocketAddr,
    tx: mpsc::Sender<Vec<u8>>,
}

impl TunRouter {
    pub fn new(device: TunDevice, config: TunConfig) -> Self {
        Self {
            device,
            config,
            nat_table: Arc::new(NatTable::new()),
            tcp_connections: Arc::new(RwLock::new(HashMap::new())),
            udp_connections: Arc::new(RwLock::new(HashMap::new())),
            next_local_port: Arc::new(std::sync::atomic::AtomicU16::new(10000)),
        }
    }
    
    /// 运行路由器
    pub async fn run(&mut self) -> Result<()> {
        tracing::info!("TUN router starting...");
        
        // 启动 NAT 清理任务
        let nat_table = self.nat_table.clone();
        tokio::spawn(async move {
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(30)).await;
                nat_table.cleanup_expired().await;
            }
        });
        
        // 主循环：读取 TUN 设备上的 IP 包
        let mut buf = vec![0u8; 65535];
        
        loop {
            match self.device.read_packet(&mut buf).await {
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
                    
                    tokio::spawn(async move {
                        if let Err(e) = Self::handle_packet(
                            packet,
                            config,
                            nat_table,
                            tcp_conns,
                            udp_conns,
                            next_port,
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
        
        Ok(())
    }
    
    /// 处理单个 IP 包
    async fn handle_packet(
        packet: Vec<u8>,
        config: TunConfig,
        nat_table: Arc<NatTable>,
        tcp_conns: Arc<RwLock<HashMap<u16, TcpProxyConnection>>>,
        udp_conns: Arc<RwLock<HashMap<u16, UdpProxyConnection>>>,
        next_port: Arc<std::sync::atomic::AtomicU16>,
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
                    tcp_conns,
                    next_port,
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
        tcp_conns: Arc<RwLock<HashMap<u16, TcpProxyConnection>>>,
        next_port: Arc<std::sync::atomic::AtomicU16>,
    ) -> Result<()> {
        // 解析 TCP 头
        let tcp = match &parsed.transport {
            Some(etherparse::TransportSlice::Tcp(tcp)) => tcp,
            _ => return Ok(()),
        };
        
        let src_port = tcp.source_port();
        let dst_port = tcp.destination_port();
        let src = SocketAddr::V4(SocketAddrV4::new(src_ip, src_port));
        let dst = SocketAddr::V4(SocketAddrV4::new(dst_ip, dst_port));
        
        tracing::debug!("TCP {} -> {} (flags: SYN={}, ACK={}, FIN={})", 
            src, dst, tcp.syn(), tcp.ack(), tcp.fin());
        
        // 检查是否是新连接 (SYN)
        if tcp.syn() && !tcp.ack() {
            // 新的 TCP 连接
            tracing::info!("New TCP connection: {} -> {}", src, dst);
            
            // 创建 NAT 条目
            nat_table.lookup_or_create(src, dst, PROTO_TCP).await;
            nat_table.update_state(src, dst, PROTO_TCP, ConnectionState::SynSent).await;
            
            // 启动代理连接
            let local_port = next_port.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            
            let (tx, mut rx) = mpsc::channel::<Vec<u8>>(1024);
            
            // 存储连接
            {
                let mut conns = tcp_conns.write().await;
                conns.insert(local_port, TcpProxyConnection {
                    src,
                    dst,
                    tx,
                });
            }
            
            // 启动代理任务
            let proxy_config = config.proxy_config.clone();
            let nat = nat_table.clone();
            
            tokio::spawn(async move {
                if let Err(e) = Self::run_tcp_proxy(
                    src,
                    dst,
                    proxy_config,
                    nat,
                    rx,
                ).await {
                    tracing::error!("TCP proxy error for {} -> {}: {}", src, dst, e);
                }
            });
        }
        
        // 获取 TCP payload
        if let Some(etherparse::TransportSlice::Tcp(tcp_slice)) = &parsed.transport {
            let payload = tcp_slice.payload();
            if !payload.is_empty() {
                // 查找对应的代理连接并发送数据
                let conns = tcp_conns.read().await;
                for (_, conn) in conns.iter() {
                    if conn.src == src && conn.dst == dst {
                        let _ = conn.tx.send(payload.to_vec()).await;
                        break;
                    }
                }
            }
        }
        
        Ok(())
    }
    
    /// 运行 TCP 代理
    async fn run_tcp_proxy(
        src: SocketAddr,
        dst: SocketAddr,
        config: Config,
        nat_table: Arc<NatTable>,
        mut rx: mpsc::Receiver<Vec<u8>>,
    ) -> Result<()> {
        tracing::debug!("Starting TCP proxy: {} -> {}", src, dst);
        
        // 建立到远程服务器的连接
        let config_arc = Arc::new(config);
        let transport = YamuxTransport::new(config_arc);
        
        let mut stream = transport.dial().await?;
        
        // 发送目标地址
        let target = format!("{}:{}\n", 
            match dst {
                SocketAddr::V4(v4) => v4.ip().to_string(),
                SocketAddr::V6(v6) => v6.ip().to_string(),
            },
            dst.port()
        );
        stream.write_all(target.as_bytes()).await?;
        
        // 更新 NAT 状态
        nat_table.update_state(src, dst, PROTO_TCP, ConnectionState::Established).await;
        
        tracing::info!("TCP proxy established: {} -> {}", src, dst);
        
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
                
                // 从远程收到数据，需要写回 TUN
                result = stream.read(&mut buf) => {
                    match result {
                        Ok(0) => {
                            tracing::debug!("Remote closed connection");
                            break;
                        }
                        Ok(n) => {
                            // TODO: 构造 IP 包写回 TUN
                            tracing::trace!("Received {} bytes from remote", n);
                        }
                        Err(e) => {
                            tracing::debug!("Read from remote failed: {}", e);
                            break;
                        }
                    }
                }
            }
        }
        
        // 更新 NAT 状态
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
