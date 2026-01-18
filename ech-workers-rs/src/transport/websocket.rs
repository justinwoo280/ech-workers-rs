/// WebSocket 传输层
/// 
/// 在 TLS 连接之上建立 WebSocket 连接

use tokio_tungstenite::WebSocketStream;
use tungstenite::protocol::Message;
use tungstenite::client::IntoClientRequest;
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
    tls_stream: S,
    host: &str,
    path: &str,
    token: Option<&str>,
) -> Result<WebSocketAdapter<S>>
where
    S: AsyncRead + AsyncWrite + Unpin,
{
    use tokio_tungstenite::client_async_with_config;
    use tungstenite::protocol::WebSocketConfig;
    
    debug!("Establishing WebSocket connection to {} (path: {})", host, path);

    // 使用 ws:// 而不是 wss://，因为 TLS 已经由 Zig tunnel 建立
    // tungstenite 只需要在已有的 TLS 流上发送 HTTP 升级请求
    let ws_url = format!("ws://{}{}", host, path);
    
    debug!("WebSocket request URL: {} (over existing TLS)", ws_url);
    debug!("WebSocket Sec-WebSocket-Protocol: {:?}", token);

    // WebSocket 配置
    let ws_config = WebSocketConfig {
        max_frame_size: Some(16 * 1024 * 1024),
        ..Default::default()
    };

    // 使用 tungstenite 的 IntoClientRequest 自动构建请求
    // 它会正确设置所有必需的 WebSocket headers
    let mut request = ws_url.into_client_request()
        .map_err(|e| Error::Protocol(format!("Failed to build request: {}", e)))?;
    
    // 添加 token 作为子协议（服务端通过此认证）
    if let Some(token) = token {
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            token.parse().unwrap()
        );
    }

    // WebSocket 握手
    let (ws_stream, response) = client_async_with_config(request, tls_stream, Some(ws_config))
        .await
        .map_err(|e| {
            tracing::error!("WebSocket handshake failed: {:?}", e);
            Error::WebSocket(e)
        })?;

    info!("✅ WebSocket handshake successful");
    debug!("WebSocket response status: {:?}", response.status());

    Ok(WebSocketAdapter::new(ws_stream))
}
