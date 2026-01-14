/// 代理服务器主逻辑

use std::sync::Arc;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{info, debug, warn, error};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::transport::yamux_optimized::{YamuxTransport, WebSocketTransport};
use super::socks5_impl::{socks5_handshake, send_target};
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
    
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                debug!("📥 New connection from {}", addr);
                let config = config.clone();
                
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, config).await {
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
async fn handle_connection(stream: TcpStream, config: Arc<Config>) -> Result<()> {
    // 检测协议类型（peek 第一个字节）
    let mut buf = [0u8; 1];
    stream.peek(&mut buf).await?;
    
    match buf[0] {
        0x05 => {
            // SOCKS5
            debug!("Detected SOCKS5 protocol");
            handle_socks5(stream, config).await
        }
        b'C' | b'G' | b'P' | b'H' => {
            // HTTP (CONNECT, GET, POST, HEAD)
            debug!("Detected HTTP protocol");
            handle_http(stream, config).await
        }
        _ => {
            warn!("Unknown protocol, first byte: 0x{:02x}", buf[0]);
            Err(Error::Protocol("Unknown protocol".into()))
        }
    }
}

/// 处理 SOCKS5 连接
async fn handle_socks5(mut local: TcpStream, config: Arc<Config>) -> Result<()> {
    // 1. SOCKS5 握手
    let target = socks5_handshake(&mut local).await?;
    info!("SOCKS5 target: {}", target.display());
    
    // 2. 建立到服务器的连接
    let remote: Box<dyn ProxyStream> = if config.use_yamux {
        // 使用 Yamux
        let transport = YamuxTransport::new(config.clone());
        let stream = transport.dial().await?;
        
        // Yamux stream 需要转换为 AsyncRead/AsyncWrite
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        Box::new(stream.compat())
    } else {
        // 简单 WebSocket
        let transport = WebSocketTransport::new(config.clone());
        let stream = transport.dial().await?;
        Box::new(stream)
    };
    
    // 3. 发送目标地址到服务器
    let mut remote = remote;
    send_target(&mut remote, &target).await?;
    
    // 4. 双向转发
    relay_bidirectional(local, remote).await?;
    
    Ok(())
}

/// 处理 HTTP CONNECT
async fn handle_http(mut local: TcpStream, config: Arc<Config>) -> Result<()> {
    // 1. 解析 CONNECT 请求
    let target = parse_connect_request(&mut local).await?;
    info!("HTTP CONNECT target: {}", target.display());
    
    // 2. 建立到服务器的连接
    let remote: Box<dyn ProxyStream> = if config.use_yamux {
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
