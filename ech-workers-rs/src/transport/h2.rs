/// HTTP/2 WebSocket Adapter (RFC 8441)
/// 
/// 将 HTTP/2 Stream 封装为 AsyncRead + AsyncWrite
/// 以便 Yamux 可以在其上运行

use std::pin::Pin;
use std::task::{Context, Poll};
use std::io;
use bytes::{Bytes, BytesMut, Buf};
use h2::client::{SendRequest, Connection};
use h2::{RecvStream, SendStream};
use tokio::io::{AsyncRead, AsyncWrite};
use tracing::{debug, info, error, warn};
use http::Request;

use crate::error::{Error, Result};
use crate::tls::TlsTunnel;

/// HTTP/2 Stream 适配器
pub struct H2StreamAdapter {
    send_stream: SendStream<Bytes>,
    recv_stream: RecvStream,
    read_buffer: BytesMut,
}

impl H2StreamAdapter {
    pub fn new(send_stream: SendStream<Bytes>, recv_stream: RecvStream) -> Self {
        Self {
            send_stream,
            recv_stream,
            read_buffer: BytesMut::new(),
        }
    }
}

impl AsyncRead for H2StreamAdapter {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        // 1. 如果缓冲区有数据，先读取缓冲区
        if !self.read_buffer.is_empty() {
            let to_read = std::cmp::min(buf.remaining(), self.read_buffer.len());
            buf.put_slice(&self.read_buffer[..to_read]);
            self.read_buffer.advance(to_read);
            return Poll::Ready(Ok(()));
        }

        // 2. 检查流是否结束
        if self.recv_stream.is_end_stream() {
            return Poll::Ready(Ok(()));
        }

        // 3. 轮询底层流
        match self.recv_stream.poll_data(cx) {
            Poll::Ready(Some(Ok(data))) => {
                // 收到新数据
                let to_read = std::cmp::min(buf.remaining(), data.len());
                buf.put_slice(&data[..to_read]);
                
                // 如果有多余数据，存入缓冲区
                if data.len() > to_read {
                    self.read_buffer.extend_from_slice(&data[to_read..]);
                }
                
                // 增加流控制窗口
                // 注意：这里简单地立即归还窗口，生产环境可能需要更精细的控制
                let _ = self.recv_stream.flow_control().release_capacity(data.len());
                
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)))
            }
            Poll::Ready(None) => {
                // 流已关闭
                Poll::Ready(Ok(()))
            }
            Poll::Pending => Poll::Pending,
        }
    }
}

impl AsyncWrite for H2StreamAdapter {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        // HTTP/2 发送数据不需要 poll_ready，但为了兼容 AsyncWrite 语义，我们检查一下 capacity
        self.send_stream.reserve_capacity(buf.len());
        
        match self.send_stream.poll_capacity(cx) {
            Poll::Ready(Some(Ok(capacity))) => {
                if capacity == 0 {
                    return Poll::Pending;
                }
                
                let to_write = std::cmp::min(capacity, buf.len());
                let data = Bytes::copy_from_slice(&buf[..to_write]);
                
                // 发送数据
                if let Err(e) = self.send_stream.send_data(data, false) {
                    return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)));
                }
                
                Poll::Ready(Ok(to_write))
            }
            Poll::Ready(Some(Err(e))) => {
                Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)))
            }
            Poll::Ready(None) => {
                // 流已关闭
                Poll::Ready(Err(io::Error::new(io::ErrorKind::BrokenPipe, "Stream closed")))
            }
            Poll::Pending => Poll::Pending,
        }
    }

    fn poll_flush(self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // HTTP/2 帧是即时发送的，不需要显式 flush
        Poll::Ready(Ok(()))
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        // 发送带 END_STREAM 标志的空数据帧
        if let Err(e) = self.send_stream.send_data(Bytes::new(), true) {
            return Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e)));
        }
        Poll::Ready(Ok(()))
    }
}

/// 建立 HTTP/2 WebSocket 连接 (RFC 8441)
pub async fn establish_h2_websocket(
    tunnel: TlsTunnel,
    host: &str,
    path: &str,
    token: Option<&str>,
) -> Result<H2StreamAdapter> {
    info!("🚀 Initializing HTTP/2 connection...");

    // 1. HTTP/2 握手
    let (mut client, h2_conn) = h2::client::handshake(tunnel).await
        .map_err(|e| Error::Protocol(format!("HTTP/2 handshake failed: {}", e)))?;
    
    // 2. 启动后台驱动任务
    tokio::spawn(async move {
        if let Err(e) = h2_conn.await {
            error!("HTTP/2 connection error: {}", e);
        }
    });

    info!("✅ HTTP/2 handshake successful");

    // 3. 构建 RFC 8441 请求
    // :method = CONNECT
    // :protocol = websocket
    // :scheme = https
    let mut builder = Request::builder()
        .method("CONNECT")
        .uri(format!("https://{}{}", host, path))
        .header("host", host) // 虽然 h2 使用 :authority，但有些库可能需要 host
        .header(":protocol", "websocket");

    // 添加 Token 到 Sec-WebSocket-Protocol
    if let Some(t) = token {
        builder = builder.header("sec-websocket-protocol", t);
    }
    
    // 添加标准 WebSocket 头
    builder = builder
        .header("sec-websocket-version", "13")
        .header("origin", format!("https://{}", host));

    let request = builder.body(())
        .map_err(|e| Error::Config(format!("Invalid request: {}", e)))?;

    info!("📤 Sending HTTP/2 WebSocket CONNECT request...");
    
    // 4. 发送请求
    let (response, stream) = client.send_request(request, false)
        .map_err(|e| Error::Protocol(format!("Failed to send request: {}", e)))?;
    
    let (head, mut body) = response.await
        .map_err(|e| Error::Protocol(format!("Failed to receive response: {}", e)))?
        .into_parts();
    
    debug!("📥 Received response status: {}", head.status);
    
    // 5. 验证响应
    if head.status != 200 {
        error!("❌ Server rejected WebSocket upgrade: {}", head.status);
        return Err(Error::Protocol(format!("Server returned status {}", head.status)));
    }
    
    info!("✅ HTTP/2 WebSocket established successfully!");
    
    // 6. 转换流
    let (recv_stream, send_stream) = stream.split();
    
    // 注意：send_request 返回的是 SendStream<Bytes>，我们需要把它和 RecvStream 组合
    // 但是 split() 给我们的是 SendStream 和 RecvStream
    // 我们需要的是 send_stream 和 recv_stream
    
    Ok(H2StreamAdapter::new(send_stream, recv_stream))
}
