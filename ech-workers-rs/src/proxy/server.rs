/// 代理服务器主逻辑

use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{info, debug, warn, error};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::transport::yamux_optimized::{YamuxTransport, WebSocketTransport};
use super::socks5_impl::{socks5_handshake_full, Socks5Request, send_target, send_udp_associate_response};
use super::http_impl::{parse_connect_request, send_connect_response};
use super::relay::relay_bidirectional;

// 定义统一的流类型 trait
trait ProxyStream: AsyncRead + AsyncWrite + Unpin + Send {}

// 为所有满足条件的类型实现 ProxyStream
impl<T> ProxyStream for T where T: AsyncRead + AsyncWrite + Unpin + Send {}

/// 运行代理服务器
pub async fn run_server(config: Config) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("🚀 Proxy server listening on {}", config.listen_addr);
    info!("   Server: {}", config.server_addr);
    info!("   ECH: {}", config.use_ech);
    info!("   Yamux: {}", config.use_yamux);
    
    let config = Arc::new(config);
    
    // 预先建立到服务器的连接（验证配置并建立 Yamux session）
    let yamux_transport = if config.use_yamux {
        info!("🔗 Pre-connecting to server...");
        let transport = Arc::new(YamuxTransport::new(config.clone()));
        
        // 触发一次连接以验证服务器可达性和 ECH 配置
        match transport.dial().await {
            Ok(stream) => {
                // 立即关闭这个 stream，只是为了验证连接
                drop(stream);
                info!("✅ Server connection verified");
            }
            Err(e) => {
                error!("❌ Failed to connect to server: {}", e);
                return Err(e);
            }
        }
        
        Some(transport)
    } else {
        None
    };
    
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                debug!("📥 New connection from {}", addr);
                let config = config.clone();
                let yamux_transport = yamux_transport.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, config, yamux_transport).await {
                        error!("Connection error from {}: {}", addr, e);
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}

/// 处理单个连接
async fn handle_connection(
    stream: TcpStream,
    config: Arc<Config>,
    yamux_transport: Option<Arc<YamuxTransport>>,
) -> Result<()> {
    // 检测协议类型（peek 第一个字节）
    let mut buf = [0u8; 1];
    stream.peek(&mut buf).await?;
    
    match buf[0] {
        0x05 => {
            // SOCKS5
            debug!("Detected SOCKS5 protocol");
            handle_socks5(stream, config, yamux_transport).await
        }
        b'C' | b'G' | b'P' | b'H' => {
            // HTTP (CONNECT, GET, POST, HEAD)
            debug!("Detected HTTP protocol");
            handle_http(stream, config, yamux_transport).await
        }
        _ => {
            warn!("Unknown protocol, first byte: 0x{:02x}", buf[0]);
            Err(Error::Protocol("Unknown protocol".into()))
        }
    }
}

/// 处理 SOCKS5 连接
async fn handle_socks5(
    mut local: TcpStream,
    config: Arc<Config>,
    yamux_transport: Option<Arc<YamuxTransport>>,
) -> Result<()> {
    // 1. SOCKS5 握手（支持 CONNECT 和 UDP ASSOCIATE）
    let request = socks5_handshake_full(&mut local).await?;
    
    match request {
        Socks5Request::Connect(target) => {
            info!("SOCKS5 CONNECT: {}", target.display());
            handle_socks5_connect(local, target, config, yamux_transport).await
        }
        Socks5Request::UdpAssociate(target) => {
            info!("SOCKS5 UDP ASSOCIATE: {}", target.display());
            handle_socks5_udp_associate(local, config).await
        }
    }
}

/// 处理 SOCKS5 CONNECT
async fn handle_socks5_connect(
    local: TcpStream,
    target: super::socks5_impl::TargetAddr,
    config: Arc<Config>,
    yamux_transport: Option<Arc<YamuxTransport>>,
) -> Result<()> {
    // 建立到服务器的连接（复用已有的 Yamux session）
    let remote: Box<dyn ProxyStream> = if let Some(transport) = yamux_transport {
        let stream = transport.dial().await?;
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        Box::new(stream.compat())
    } else if config.use_yamux {
        let transport = YamuxTransport::new(config.clone());
        let stream = transport.dial().await?;
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        Box::new(stream.compat())
    } else {
        let transport = WebSocketTransport::new(config.clone());
        let stream = transport.dial().await?;
        Box::new(stream)
    };
    
    // 发送目标地址到服务器
    let mut remote = remote;
    send_target(&mut remote, &target).await?;
    
    // 双向转发
    relay_bidirectional(local, remote).await?;
    
    Ok(())
}

/// 处理 SOCKS5 UDP ASSOCIATE
async fn handle_socks5_udp_associate(
    mut tcp_control: TcpStream,
    config: Arc<Config>,
) -> Result<()> {
    use tokio::net::UdpSocket;
    
    // 1. 创建 UDP socket 用于接收客户端的 UDP 数据
    let udp_socket = UdpSocket::bind("0.0.0.0:0").await?;
    let local_addr = udp_socket.local_addr()?;
    
    debug!("UDP relay socket bound to: {}", local_addr);
    
    // 2. 发送 UDP ASSOCIATE 响应（告知客户端 UDP relay 地址）
    send_udp_associate_response(&mut tcp_control, local_addr).await?;
    
    // 3. 启动 UDP relay
    let udp_socket = Arc::new(udp_socket);
    let udp_socket_clone = udp_socket.clone();
    
    // UDP 数据转发任务
    let _config_clone = config.clone();
    let relay_task = tokio::spawn(async move {
        let mut buf = vec![0u8; 65535];
        let mut client_addr: Option<std::net::SocketAddr> = None;
        
        loop {
            match udp_socket_clone.recv_from(&mut buf).await {
                Ok((n, addr)) => {
                    // 记录客户端地址
                    if client_addr.is_none() {
                        client_addr = Some(addr);
                        debug!("UDP client connected from: {}", addr);
                    }
                    
                    // 解析 SOCKS5 UDP 帧
                    if n < 10 {
                        continue;
                    }
                    
                    // 跳过 RSV (2) + FRAG (1)
                    let atyp = buf[3];
                    let (target_addr, data_start) = match atyp {
                        0x01 => {
                            // IPv4
                            if n < 10 { continue; }
                            let ip = std::net::Ipv4Addr::new(buf[4], buf[5], buf[6], buf[7]);
                            let port = u16::from_be_bytes([buf[8], buf[9]]);
                            (format!("{}:{}", ip, port), 10)
                        }
                        0x03 => {
                            // Domain
                            let len = buf[4] as usize;
                            if n < 7 + len { continue; }
                            let domain = String::from_utf8_lossy(&buf[5..5+len]).to_string();
                            let port = u16::from_be_bytes([buf[5+len], buf[6+len]]);
                            (format!("{}:{}", domain, port), 7 + len)
                        }
                        _ => continue,
                    };
                    
                    let data = &buf[data_start..n];
                    debug!("UDP relay: {} -> {} ({} bytes)", addr, target_addr, data.len());
                    
                    // TODO: 通过代理转发 UDP 数据
                    // 目前简单地直接发送（需要服务端支持 UDP）
                    // 这里可以扩展为通过 TCP 隧道转发
                }
                Err(e) => {
                    debug!("UDP recv error: {}", e);
                    break;
                }
            }
        }
    });
    
    // 4. 等待 TCP 控制连接关闭
    // 当 TCP 连接关闭时，UDP 会话也应该结束
    let mut buf = [0u8; 1];
    loop {
        match tokio::io::AsyncReadExt::read(&mut tcp_control, &mut buf).await {
            Ok(0) => {
                debug!("SOCKS5 UDP ASSOCIATE: TCP control connection closed");
                break;
            }
            Err(_) => break,
            _ => continue,
        }
    }
    
    // 取消 relay 任务
    relay_task.abort();
    
    Ok(())
}

/// 处理 HTTP CONNECT
async fn handle_http(
    mut local: TcpStream,
    config: Arc<Config>,
    yamux_transport: Option<Arc<YamuxTransport>>,
) -> Result<()> {
    // 1. 解析 CONNECT 请求
    let target = parse_connect_request(&mut local).await?;
    info!("HTTP CONNECT target: {}", target.display());
    
    // 2. 建立到服务器的连接（复用已有的 Yamux session）
    let remote: Box<dyn ProxyStream> = if let Some(transport) = yamux_transport {
        let stream = transport.dial().await?;
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        Box::new(stream.compat())
    } else if config.use_yamux {
        let transport = YamuxTransport::new(config.clone());
        let stream = transport.dial().await?;
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        Box::new(stream.compat())
    } else {
        let transport = WebSocketTransport::new(config.clone());
        let stream = transport.dial().await?;
        Box::new(stream)
    };
    
    // 3. 发送目标地址到服务器
    let mut remote = remote;
    send_target(&mut remote, &target).await?;
    
    // 4. 发送 200 响应给客户端
    send_connect_response(&mut local).await?;
    
    // 5. 双向转发
    relay_bidirectional(local, remote).await?;
    
    Ok(())
}
