/// SOCKS5 协议实现
/// 
/// RFC 1928: https://www.rfc-editor.org/rfc/rfc1928

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::stream::ConnectionContext;
// use crate::transport::Transport;

/// SOCKS5 版本号
const SOCKS5_VERSION: u8 = 0x05;

/// 认证方法
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum AuthMethod {
    NoAuth = 0x00,
    // GSSAPI = 0x01,
    // UsernamePassword = 0x02,
    NoAcceptable = 0xFF,
}

/// SOCKS5 命令
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum Command {
    Connect = 0x01,
    Bind = 0x02,
    UdpAssociate = 0x03,
}

/// 地址类型
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum AddressType {
    IPv4 = 0x01,
    Domain = 0x03,
    IPv6 = 0x04,
}

/// 回复状态
#[repr(u8)]
#[derive(Debug, Clone, Copy)]
enum Reply {
    Succeeded = 0x00,
    GeneralFailure = 0x01,
    ConnectionNotAllowed = 0x02,
    NetworkUnreachable = 0x03,
    HostUnreachable = 0x04,
    ConnectionRefused = 0x05,
    TtlExpired = 0x06,
    CommandNotSupported = 0x07,
    AddressTypeNotSupported = 0x08,
}

/// 处理 SOCKS5 连接
pub async fn handle_socks5(
    mut client: TcpStream,
    first_byte: u8,
    transport: Arc<Transport>,
) -> Result<()> {
    // 1. 握手阶段
    let auth_method = handshake(&mut client, first_byte).await?;
    debug!("SOCKS5 handshake completed, auth method: {:?}", auth_method);

    // 2. 认证阶段（如果需要）
    if let AuthMethod::NoAuth = auth_method {
        // 无需认证
    } else {
        return Err(Error::Protocol("Authentication not supported".to_string()));
    }

    // 3. 请求阶段
    let (command, target) = read_request(&mut client).await?;
    debug!("SOCKS5 request: {:?} to {}", command, target);

    // 4. 只支持 CONNECT 命令
    if !matches!(command, Command::Connect) {
        send_reply(&mut client, Reply::CommandNotSupported, "0.0.0.0:0").await?;
        return Err(Error::Protocol(format!("Unsupported command: {:?}", command)));
    }

    // 5. 建立隧道连接
    info!("Connecting to {}", target);
    let server_ctx = match transport.dial().await {
        Ok(ctx) => ctx,
        Err(e) => {
            warn!("Failed to dial: {}", e);
            send_reply(&mut client, Reply::GeneralFailure, "0.0.0.0:0").await?;
            return Err(e);
        }
    };

    // 6. 发送成功响应
    send_reply(&mut client, Reply::Succeeded, "0.0.0.0:0").await?;
    info!("✅ SOCKS5 tunnel established to {}", target);

    // 7. 双向转发数据
    relay_data(client, server_ctx, &target).await
}

/// SOCKS5 握手
async fn handshake(client: &mut TcpStream, first_byte: u8) -> Result<AuthMethod> {
    // 第一个字节已经读取（版本号）
    if first_byte != SOCKS5_VERSION {
        return Err(Error::Protocol(format!("Invalid SOCKS version: {}", first_byte)));
    }

    // 读取认证方法数量
    let nmethods = client.read_u8().await?;
    
    // 读取认证方法列表
    let mut methods = vec![0u8; nmethods as usize];
    client.read_exact(&mut methods).await?;

    // 检查是否支持无认证
    let auth_method = if methods.contains(&(AuthMethod::NoAuth as u8)) {
        AuthMethod::NoAuth
    } else {
        AuthMethod::NoAcceptable
    };

    // 发送响应
    client.write_all(&[SOCKS5_VERSION, auth_method as u8]).await?;
    client.flush().await?;

    if matches!(auth_method, AuthMethod::NoAcceptable) {
        return Err(Error::Protocol("No acceptable auth method".to_string()));
    }

    Ok(auth_method)
}

/// 读取 SOCKS5 请求
async fn read_request(client: &mut TcpStream) -> Result<(Command, String)> {
    // 读取请求头
    let mut header = [0u8; 4];
    client.read_exact(&mut header).await?;

    let version = header[0];
    let cmd = header[1];
    // let _reserved = header[2];
    let atyp = header[3];

    if version != SOCKS5_VERSION {
        return Err(Error::Protocol(format!("Invalid version: {}", version)));
    }

    let command = match cmd {
        0x01 => Command::Connect,
        0x02 => Command::Bind,
        0x03 => Command::UdpAssociate,
        _ => return Err(Error::Protocol(format!("Unknown command: {}", cmd))),
    };

    // 读取目标地址
    let target = match atyp {
        0x01 => {
            // IPv4
            let mut addr = [0u8; 4];
            client.read_exact(&mut addr).await?;
            let port = client.read_u16().await?;
            format!("{}.{}.{}.{}:{}", addr[0], addr[1], addr[2], addr[3], port)
        }
        0x03 => {
            // 域名
            let len = client.read_u8().await?;
            let mut domain = vec![0u8; len as usize];
            client.read_exact(&mut domain).await?;
            let port = client.read_u16().await?;
            format!("{}:{}", String::from_utf8_lossy(&domain), port)
        }
        0x04 => {
            // IPv6
            let mut addr = [0u8; 16];
            client.read_exact(&mut addr).await?;
            let port = client.read_u16().await?;
            format!("[{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}:{:02x}{:02x}]:{}",
                addr[0], addr[1], addr[2], addr[3], addr[4], addr[5], addr[6], addr[7],
                addr[8], addr[9], addr[10], addr[11], addr[12], addr[13], addr[14], addr[15], port)
        }
        _ => return Err(Error::Protocol(format!("Unknown address type: {}", atyp))),
    };

    Ok((command, target))
}

/// 发送 SOCKS5 响应
async fn send_reply(client: &mut TcpStream, reply: Reply, _bind_addr: &str) -> Result<()> {
    // 简化：总是返回 0.0.0.0:0
    let response = [
        SOCKS5_VERSION,
        reply as u8,
        0x00, // Reserved
        0x01, // IPv4
        0, 0, 0, 0, // 0.0.0.0
        0, 0, // Port 0
    ];

    client.write_all(&response).await?;
    client.flush().await?;

    Ok(())
}

/// 双向数据转发
async fn relay_data(
    mut client: TcpStream,
    mut server: ConnectionContext,
    target: &str,
) -> Result<()> {
    use tokio::io::copy_bidirectional;

    match copy_bidirectional(&mut client, &mut server.stream).await {
        Ok((client_to_server, server_to_client)) => {
            info!(
                "SOCKS5 connection to {} closed: ↑ {} bytes, ↓ {} bytes",
                target, client_to_server, server_to_client
            );
            Ok(())
        }
        Err(e) => {
            warn!("SOCKS5 relay error for {}: {}", target, e);
            Err(e.into())
        }
    }
}
