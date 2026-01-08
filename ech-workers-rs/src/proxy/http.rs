/// HTTP CONNECT 协议实现
/// 
/// RFC 7231 Section 4.3.6: https://www.rfc-editor.org/rfc/rfc7231#section-4.3.6

use tokio::net::TcpStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::{debug, info, warn};
use std::sync::Arc;

use crate::error::{Error, Result};
use crate::stream::ConnectionContext;
// use crate::transport::Transport;

/// 处理 HTTP CONNECT 请求
pub async fn handle_http_connect(
    mut client: TcpStream,
    first_byte: u8,
    transport: Arc<Transport>,
) -> Result<()> {
    // 1. 读取 HTTP 请求头
    let (method, target) = parse_http_request(&mut client, first_byte).await?;
    
    // 2. 只支持 CONNECT 方法
    if method != "CONNECT" {
        send_error_response(&mut client, 405, "Method Not Allowed").await?;
        return Err(Error::Protocol(format!("Unsupported HTTP method: {}", method)));
    }

    debug!("HTTP CONNECT to {}", target);

    // 3. 建立隧道连接
    info!("Connecting to {}", target);
    let server_ctx = match transport.dial().await {
        Ok(ctx) => ctx,
        Err(e) => {
            warn!("Failed to dial: {}", e);
            send_error_response(&mut client, 502, "Bad Gateway").await?;
            return Err(e);
        }
    };

    // 4. 发送 200 Connection Established
    send_success_response(&mut client).await?;
    info!("✅ HTTP CONNECT tunnel established to {}", target);

    // 5. 双向转发数据
    relay_data(client, server_ctx, &target).await
}

/// 解析 HTTP 请求
async fn parse_http_request(
    client: &mut TcpStream,
    first_byte: u8,
) -> Result<(String, String)> {
    // 读取请求行和头部（最多 8KB）
    let mut buffer = vec![first_byte];
    buffer.reserve(8192);

    // 读取直到 \r\n\r\n
    let mut headers_complete = false;
    while !headers_complete && buffer.len() < 8192 {
        let mut byte = [0u8; 1];
        client.read_exact(&mut byte).await?;
        buffer.push(byte[0]);

        // 检查是否到达头部结束
        if buffer.len() >= 4 {
            let len = buffer.len();
            if &buffer[len-4..len] == b"\r\n\r\n" {
                headers_complete = true;
            }
        }
    }

    if !headers_complete {
        return Err(Error::Protocol("HTTP headers too large or incomplete".to_string()));
    }

    // 使用 httparse 解析
    let mut headers = [httparse::EMPTY_HEADER; 64];
    let mut req = httparse::Request::new(&mut headers);

    match req.parse(&buffer) {
        Ok(httparse::Status::Complete(_)) => {
            let method = req.method.ok_or_else(|| Error::Protocol("No method".to_string()))?;
            let path = req.path.ok_or_else(|| Error::Protocol("No path".to_string()))?;

            Ok((method.to_string(), path.to_string()))
        }
        Ok(httparse::Status::Partial) => {
            Err(Error::Protocol("Incomplete HTTP request".to_string()))
        }
        Err(e) => {
            Err(Error::Protocol(format!("Failed to parse HTTP request: {}", e)))
        }
    }
}

/// 发送成功响应
async fn send_success_response(client: &mut TcpStream) -> Result<()> {
    let response = b"HTTP/1.1 200 Connection Established\r\n\r\n";
    client.write_all(response).await?;
    client.flush().await?;
    Ok(())
}

/// 发送错误响应
async fn send_error_response(
    client: &mut TcpStream,
    status_code: u16,
    reason: &str,
) -> Result<()> {
    let response = format!(
        "HTTP/1.1 {} {}\r\nContent-Length: 0\r\n\r\n",
        status_code, reason
    );
    client.write_all(response.as_bytes()).await?;
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
                "HTTP CONNECT to {} closed: ↑ {} bytes, ↓ {} bytes",
                target, client_to_server, server_to_client
            );
            Ok(())
        }
        Err(e) => {
            warn!("HTTP CONNECT relay error for {}: {}", target, e);
            Err(e.into())
        }
    }
}
