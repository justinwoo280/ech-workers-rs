/// WebSocket 传输层
/// 
/// 在 TLS 连接之上建立 WebSocket 连接

use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Message;
use tracing::{debug, info};
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
        use futures::stream::StreamExt;
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
        use futures::sink::SinkExt;
        
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
        use futures::sink::SinkExt;
        match futures::sink::SinkExt::poll_flush_unpin(&mut self.inner, cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        use futures::sink::SinkExt;
        match futures::sink::SinkExt::poll_close_unpin(&mut self.inner, cx) {
            Poll::Ready(Ok(())) => Poll::Ready(Ok(())),
            Poll::Ready(Err(e)) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Pending => Poll::Pending,
        }
    }
}

/// 建立 WebSocket 连接（通过已有的 TLS 连接）
pub async fn establish_websocket_over_tls<S>(
    tls_stream: S,
    url: &str,
    token: Option<&str>,
) -> Result<WebSocketAdapter<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use tokio_tungstenite::client_async;
    
    debug!("Establishing WebSocket connection to {}", url);

    // 构建请求
    let mut request = http::Request::builder()
        .uri(url)
        .header("Host", url.split("://").nth(1).unwrap_or("localhost").split('/').next().unwrap_or("localhost"))
        .header("Upgrade", "websocket")
        .header("Connection", "Upgrade")
        .header("Sec-WebSocket-Version", "13")
        .header("Sec-WebSocket-Key", tungstenite::handshake::client::generate_key());

    // 添加 token
    if let Some(token) = token {
        request = request.header("Sec-WebSocket-Protocol", token);
    }

    let request = request
        .body(())
        .map_err(|e| Error::Protocol(format!("Failed to build request: {}", e)))?;

    // WebSocket 握手
    let (ws_stream, response) = client_async(request, tls_stream)
        .await
        .map_err(|e| Error::WebSocket(e))?;

    info!("✅ WebSocket handshake successful");
    debug!("WebSocket response status: {:?}", response.status());

    Ok(WebSocketAdapter::new(ws_stream))
}
