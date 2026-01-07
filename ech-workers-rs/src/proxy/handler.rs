/// 连接处理器 - L7 核心逻辑
/// 
/// 这个模块完全类型盲态，只使用 OpaqueStream

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{info, debug, error};
use std::sync::Arc;

use crate::error::{Error, Result};
// use crate::transport::Transport;

/// 处理客户端连接
/// 
/// 这是 L7 的入口点，完全类型盲态
pub async fn handle_connection(
    mut client: TcpStream,
    transport: Arc<Transport>,
) -> Result<()> {
    // 1. 读取第一个字节判断协议
    let mut first_byte = [0u8; 1];
    client.read_exact(&mut first_byte).await?;

    match first_byte[0] {
        0x05 => {
            // SOCKS5
            debug!("Detected SOCKS5 protocol");
            handle_socks5(client, first_byte[0], transport).await
        }
        b'C' | b'G' | b'P' | b'D' | b'H' | b'O' | b'T' => {
            // HTTP (CONNECT, GET, POST, DELETE, HEAD, OPTIONS, TRACE)
            debug!("Detected HTTP protocol");
            handle_http(client, first_byte[0], transport).await
        }
        _ => {
            error!("Unknown protocol, first byte: 0x{:02x}", first_byte[0]);
            Err(Error::Protocol(format!("Unknown protocol: 0x{:02x}", first_byte[0])))
        }
    }
}

/// 处理 SOCKS5 连接
async fn handle_socks5(
    client: TcpStream,
    first_byte: u8,
    transport: Arc<Transport>,
) -> Result<()> {
    // 调用 socks5 模块的实现
    super::socks5::handle_socks5(client, first_byte, transport).await
}

/// 处理 HTTP CONNECT
async fn handle_http(
    client: TcpStream,
    first_byte: u8,
    transport: Arc<Transport>,
) -> Result<()> {
    // 调用 http 模块的实现
    super::http::handle_http_connect(client, first_byte, transport).await
}
