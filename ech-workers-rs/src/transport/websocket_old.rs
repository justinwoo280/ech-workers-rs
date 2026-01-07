/// WebSocket 传输层
/// 
/// 在 TLS 连接之上建立 WebSocket 连接

use tokio::net::TcpStream;
use tokio_rustls::client::TlsStream;
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream, connect_async};
use tungstenite::protocol::{WebSocketConfig, Message};
use tracing::{debug, info};
use url::Url;
use tokio::io::{AsyncRead, AsyncWrite};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::io;

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
        use futures::StreamExt;
        match Pin::new(&mut self.inner).poll_next(cx) {
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
        use futures::SinkExt;
        
        let msg = Message::Binary(buf.to_vec());
        match Pin::new(&mut self.inner).poll_ready(cx) {
            Poll::Ready(Ok(())) => {
                match Pin::new(&mut self.inner).start_send(msg) {
                    Ok(()) => Poll::Ready(Ok(buf.len())),
                    Err(e) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
                }
            }
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        use futures::SinkExt;
        match Pin::new(&mut self.inner).poll_flush(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        use futures::SinkExt;
        match Pin::new(&mut self.inner).poll_close(cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// 建立 WebSocket 连接
pub async fn establish_websocket(
    url: &str,
    token: Option<&str>,
) -> Result<WebSocketAdapter<MaybeTlsStream<TcpStream>>> {
    debug!("Establishing WebSocket connection to {}", url);

    // 构建请求
    let mut request = http::Request::builder()
        .uri(url)
        .body(())
        .map_err(|e| Error::Protocol(format!("Invalid request: {}", e)))?;

    // 添加 token
    if let Some(token) = token {
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            token.parse().map_err(|e| Error::Protocol(format!("Invalid token: {}", e)))?
        );
    }

    let request = request
        .body(())
        .map_err(|e| Error::Protocol(format!("Failed to build WebSocket request: {}", e)))?;

    // WebSocket 配置
    let ws_config = WebSocketConfig {
        max_message_size: Some(64 << 20), // 64 MB
        max_frame_size: Some(16 << 20),   // 16 MB
        ..Default::default()
    };

    // 执行 WebSocket 握手
    let (ws_stream, response) = client_async_tls_with_config(
        request,
        tls_stream,
        Some(ws_config),
        None, // 不使用额外的 TLS connector，因为已经是 TLS 流了
    )
    .await
    .map_err(|e| Error::WebSocket(e))?;

    info!("✅ WebSocket handshake successful");
    debug!("WebSocket response status: {}", response.status());

    // ========== 关键转换点 ==========
    // 1. WsStream 使用 futures::io traits
    let ws_io = WsStream::new(ws_stream);
    
    // 2. compat() 转换为 tokio::io traits
    let tokio_io = ws_io.compat();
    
    // 3. Box 类型擦除，隐藏所有内部类型
    let boxed: Box<dyn Io> = Box::new(tokio_io);
    
    Ok(boxed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_url_parsing() {
        let url = "wss://example.com:443/ws";
        let parsed = Url::parse(url).unwrap();
        assert_eq!(parsed.scheme(), "wss");
        assert_eq!(parsed.host_str(), Some("example.com"));
        assert_eq!(parsed.port(), Some(443));
        assert_eq!(parsed.path(), "/ws");
    }
}
