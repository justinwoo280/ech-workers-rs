/// WebSocket 传输层
/// 
/// 在 TLS 连接之上建立 WebSocket 连接

use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Message;
use tracing::{debug, info, error};
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::io;
use base64::Engine;

use crate::error::{Error, Result};

/// WebSocket 适配器 - 将 WebSocketStream 转换为 AsyncRead/AsyncWrite
pub struct WebSocketAdapter<S> {
    inner: WebSocketStream<S>,
    read_buffer: Vec<u8>,
    read_pos: usize,
}

impl<S> WebSocketAdapter<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    pub fn new(stream: WebSocketStream<S>) -> Self {
        Self {
            inner: stream,
            read_buffer: Vec::new(),
            read_pos: 0,
        }
    }
}

impl<S> AsyncRead for WebSocketAdapter<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // 如果有缓冲数据，先读取
        if self.read_pos < self.read_buffer.len() {
            let remaining = &self.read_buffer[self.read_pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.read_pos += to_copy;
            
            if self.read_pos >= self.read_buffer.len() {
                self.read_buffer.clear();
                self.read_pos = 0;
            }
            
            return Poll::Ready(Ok(()));
        }

        // 读取新的 WebSocket 消息
        match futures::stream::StreamExt::poll_next_unpin(&mut self.inner, cx) {
            Poll::Ready(Some(Ok(msg))) => {
                match msg {
                    Message::Binary(data) => {
                        let to_copy = data.len().min(buf.remaining());
                        buf.put_slice(&data[..to_copy]);
                        
                        if to_copy < data.len() {
                            self.read_buffer = data;
                            self.read_pos = to_copy;
                        }
                        
                        Poll::Ready(Ok(()))
                    }
                    Message::Close(_) => Poll::Ready(Ok(())),
                    _ => {
                        // 忽略其他消息类型，继续读取
                        cx.waker().wake_by_ref();
                        Poll::Pending
                    }
                }
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)))
            }
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<S> AsyncWrite for WebSocketAdapter<S>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        let msg = Message::Binary(buf.to_vec());
        match futures::sink::SinkExt::poll_ready_unpin(&mut self.inner, cx) {
            Poll::Ready(Ok(())) => {
                match futures::sink::SinkExt::start_send_unpin(&mut self.inner, msg) {
                    Ok(()) => Poll::Ready(Ok(buf.len())),
                    Err(e) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match futures::sink::SinkExt::poll_flush_unpin(&mut self.inner, cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match futures::sink::SinkExt::poll_close_unpin(&mut self.inner, cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// 建立 WebSocket 连接（通过已有的 TLS 连接）
/// 
/// # 参数
/// - `tls_stream`: 已建立的 TLS 连接
/// - `host`: 服务器主机名（用于 Host header）
/// - `path`: 请求路径（如 "/" 或 "/ws"）
/// - `token`: 认证 token（通过 Sec-WebSocket-Protocol 发送）
pub async fn establish_websocket_over_tls<S>(
    mut tls_stream: S,
    host: &str,
    path: &str,
    token: Option<&str>,
) -> Result<WebSocketAdapter<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    
    debug!("Establishing WebSocket connection to {} (path: {})", host, path);

    // 手动构建 WebSocket 升级请求
    // 生成随机的 Sec-WebSocket-Key
    let random_bytes: [u8; 16] = rand::random();
    let ws_key = base64::engine::general_purpose::STANDARD.encode(&random_bytes);
    
    // 构建 HTTP 升级请求
    let mut request = format!(
        "GET {} HTTP/1.1\r\n\
         Host: {}\r\n\
         Upgrade: websocket\r\n\
         Connection: Upgrade\r\n\
         Sec-WebSocket-Key: {}\r\n\
         Sec-WebSocket-Version: 13\r\n",
        path, host, ws_key
    );
    
    // 添加 token 作为子协议
    if let Some(token) = token {
        request.push_str(&format!("Sec-WebSocket-Protocol: {}\r\n", token));
    }
    request.push_str("\r\n");
    
    info!("📤 Sending WebSocket upgrade request:");
    for line in request.lines().take(6) {
        debug!("   > {}", line);
    }
    
    // 发送请求
    tls_stream.write_all(request.as_bytes()).await
        .map_err(|e| Error::Io(e))?;
    tls_stream.flush().await
        .map_err(|e| Error::Io(e))?;
    
    // 读取响应头
    let mut response_buf = vec![0u8; 4096];
    let n = tls_stream.read(&mut response_buf).await
        .map_err(|e| Error::Io(e))?;
    
    let response_data = &response_buf[..n];
    
    // 打印原始响应的前 200 字节用于调试
    info!("📥 Received {} bytes from server", n);
    if let Ok(text) = std::str::from_utf8(response_data) {
        for line in text.lines().take(5) {
            debug!("   < {}", line);
        }
    } else {
        // 如果不是有效 UTF-8，打印十六进制
        let hex: String = response_data.iter().take(64).map(|b| format!("{:02x} ", b)).collect();
        error!("   < (binary) {}", hex);
    }
    
    // 检查是否是有效的 HTTP 101 响应
    let response_str = String::from_utf8_lossy(response_data);
    if !response_str.starts_with("HTTP/1.1 101") {
        error!("❌ Server did not return HTTP/1.1 101 Switching Protocols");
        error!("   Response: {}", response_str.lines().next().unwrap_or("(empty)"));
        return Err(Error::Protocol(format!(
            "WebSocket upgrade failed: {}", 
            response_str.lines().next().unwrap_or("(empty)")
        )));
    }
    
    info!("✅ WebSocket upgrade accepted");
    
    // 使用 tokio-tungstenite 包装已升级的连接
    use tokio_tungstenite::WebSocketStream;
    use tungstenite::protocol::{WebSocketConfig, Role};
    
    let ws_config = WebSocketConfig {
        max_frame_size: Some(16 * 1024 * 1024),
        ..Default::default()
    };
    
    let ws_stream = WebSocketStream::from_raw_socket(tls_stream, Role::Client, Some(ws_config)).await;

    info!("✅ WebSocket handshake successful");

    Ok(WebSocketAdapter::new(ws_stream))
}
