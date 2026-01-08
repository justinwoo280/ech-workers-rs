# 关键代码片段

## 1. 完整的连接流程

```rust
// 客户端 → 服务器的完整流程
async fn handle_socks5(mut local: TcpStream, config: Arc<Config>) -> Result<()> {
    // 1. SOCKS5 握手，获取目标地址
    let target = socks5_handshake(&mut local).await?;
    
    // 2. 建立到服务器的连接
    //    DoH → ECH → Zig TLS → WebSocket → Yamux
    let transport = YamuxTransport::new(config.clone());
    let mut remote = transport.dial().await?;
    
    // 3. 发送目标地址到服务器（SOCKS5 格式）
    send_target(&mut remote, &target).await?;
    
    // 4. 双向转发数据
    relay_bidirectional(local, remote).await?;
    
    Ok(())
}
```

## 2. ECH 配置查询和验证

```rust
// DoH 查询 ECH 配置
let ech_config = ech::query_ech_config(
    "crypto.cloudflare.com",
    "https://cloudflare-dns.com/dns-query"
).await?;

// 创建 TLS 配置
let config = TunnelConfig::new("crypto.cloudflare.com", 443)
    .with_ech(ech_config, true);  // enforce_ech = true

// 建立连接
let tunnel = TlsTunnel::connect(config)?;

// 验证 ECH 是否被接受
let info = tunnel.info()?;
if !info.used_ech {
    return Err(Error::EchNotAccepted);
}
```

## 3. Yamux 会话管理

```rust
// 后台任务管理会话
async fn session_manager_task(...) {
    let mut session: Option<YamuxConnection> = None;
    let mut consecutive_failures = 0;
    
    while let Some(command) = command_rx.recv().await {
        match command {
            SessionCommand::OpenStream(response_tx) => {
                // 尝试打开 stream，失败时自动重连
                let result = open_stream_with_retry(
                    &mut session,
                    &config,
                    &mut consecutive_failures,
                    MAX_FAILURES,
                ).await;
                
                let _ = response_tx.send(result);
            }
            SessionCommand::HealthCheck(response_tx) => {
                let is_healthy = session.as_ref()
                    .map(|s| !s.is_closed())
                    .unwrap_or(false);
                let _ = response_tx.send(is_healthy);
            }
            SessionCommand::Shutdown => break,
        }
    }
}
```

## 4. SOCKS5 地址序列化

```rust
impl TargetAddr {
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut buf = Vec::new();
        
        match self {
            TargetAddr::Domain(domain, port) => {
                buf.push(0x03);  // Domain type
                buf.push(domain.len() as u8);
                buf.extend_from_slice(domain.as_bytes());
                buf.extend_from_slice(&port.to_be_bytes());  // Big-Endian!
            }
            TargetAddr::Ipv4(ip, port) => {
                buf.push(0x01);  // IPv4 type
                buf.extend_from_slice(&ip.octets());
                buf.extend_from_slice(&port.to_be_bytes());
            }
            TargetAddr::Ipv6(ip, port) => {
                buf.push(0x04);  // IPv6 type
                buf.extend_from_slice(&ip.octets());
                buf.extend_from_slice(&port.to_be_bytes());
            }
        }
        
        buf
    }
}
```

## 5. 缓冲数据转发

```rust
async fn relay_with_buffer<R, W>(...) -> Result<u64> {
    let mut buffer = vec![0u8; 32 * 1024];  // 32KB
    let mut total_bytes = 0u64;
    
    loop {
        // 读取数据（带超时）
        let n = match timeout(Duration::from_secs(300), reader.read(&mut buffer)).await {
            Ok(Ok(0)) => break,  // EOF
            Ok(Ok(n)) => n,
            Ok(Err(e)) => return Err(e.into()),
            Err(_) => break,  // Timeout
        };
        
        // 写入数据
        writer.write_all(&buffer[..n]).await?;
        
        // 定期 flush（避免小包堆积）
        if total_bytes % (16 * 1024) == 0 {
            writer.flush().await?;
        }
        
        total_bytes += n as u64;
    }
    
    // 最终 flush 和半关闭
    writer.flush().await?;
    writer.shutdown().await?;
    
    Ok(total_bytes)
}
```

## 6. WebSocket 适配器

```rust
impl<S> AsyncRead for WebSocketAdapter<S> {
    fn poll_read(...) -> Poll<io::Result<()>> {
        // 1. 先读取缓冲区
        if self.read_pos < self.read_buffer.len() {
            let remaining = &self.read_buffer[self.read_pos..];
            let to_copy = remaining.len().min(buf.remaining());
            buf.put_slice(&remaining[..to_copy]);
            self.read_pos += to_copy;
            return Poll::Ready(Ok(()));
        }

        // 2. 读取新的 WebSocket 消息
        match poll_next_unpin(&mut self.inner, cx) {
            Poll::Ready(Some(Ok(Message::Binary(data)))) => {
                let to_copy = data.len().min(buf.remaining());
                buf.put_slice(&data[..to_copy]);
                
                // 如果数据太大，缓存剩余部分
                if to_copy < data.len() {
                    self.read_buffer = data;
                    self.read_pos = to_copy;
                }
                
                Poll::Ready(Ok(()))
            }
            Poll::Ready(Some(Ok(Message::Close(_)))) => Poll::Ready(Ok(())),
            Poll::Ready(Some(Err(e))) => Poll::Ready(Err(io::Error::new(io::ErrorKind::Other, e))),
            Poll::Ready(None) => Poll::Ready(Ok(())),
            Poll::Pending => Poll::Pending,
        }
    }
}
```

## 7. 协议自动检测

```rust
async fn handle_connection(mut stream: TcpStream, config: Arc<Config>) -> Result<()> {
    // Peek 第一个字节，不消耗数据
    let mut buf = [0u8; 1];
    stream.peek(&mut buf).await?;
    
    match buf[0] {
        0x05 => {
            // SOCKS5: 版本号是 0x05
            debug!("Detected SOCKS5 protocol");
            handle_socks5(stream, config).await
        }
        b'C' | b'G' | b'P' | b'H' => {
            // HTTP: CONNECT, GET, POST, HEAD
            debug!("Detected HTTP protocol");
            handle_http(stream, config).await
        }
        _ => {
            warn!("Unknown protocol, first byte: 0x{:02x}", buf[0]);
            Err(Error::Protocol("Unknown protocol".into()))
        }
    }
}
```

## 8. FFI 安全包装

```rust
pub struct TlsTunnel {
    inner: *mut ffi::TlsTunnel,
    _host: CString,              // 保持所有权
    _ech_config: Option<Vec<u8>>, // 保持所有权
}

impl TlsTunnel {
    pub fn connect(config: TunnelConfig) -> Result<Self> {
        // 转换 Rust 类型到 C 类型
        let host_cstr = CString::new(config.host.as_str())?;
        
        let c_config = ffi::TlsTunnelConfig {
            host: host_cstr.as_ptr(),
            port: config.port,
            ech_config: config.ech_config.as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null()),
            ech_config_len: config.ech_config.as_ref()
                .map(|v| v.len())
                .unwrap_or(0),
            enforce_ech: config.enforce_ech,
            // ...
        };
        
        // 调用 C API
        let mut error = ffi::TlsError::Success;
        let tunnel = unsafe {
            ffi::tls_tunnel_create(&c_config, &mut error)
        };
        
        if tunnel.is_null() {
            return Err(Error::from(error));
        }
        
        Ok(Self {
            inner: tunnel,
            _host: host_cstr,           // 保持所有权
            _ech_config: config.ech_config, // 保持所有权
        })
    }
}

// RAII: 自动清理
impl Drop for TlsTunnel {
    fn drop(&mut self) {
        unsafe {
            ffi::tls_tunnel_destroy(self.inner);
        }
    }
}
```

## 9. 错误处理模式

```rust
// 使用 thiserror 定义错误类型
#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    
    #[error("ECH not accepted (possible downgrade attack)")]
    EchNotAccepted,
    
    #[error("Protocol error: {0}")]
    Protocol(String),
    
    // ...
}

// 使用 Result<T> 类型别名
pub type Result<T> = std::result::Result<T, Error>;

// 错误传播
async fn some_function() -> Result<()> {
    let data = read_data().await?;  // ? 自动转换错误
    process_data(data)?;
    Ok(())
}
```

## 10. 日志和调试

```rust
use tracing::{info, debug, warn, error, trace};

// 不同级别的日志
info!("🚀 Proxy server listening on {}", addr);
debug!("Establishing TLS connection to {}:{}", host, port);
warn!("Failed to open stream: {}", e);
error!("Connection error: {}", e);
trace!("Sending {} bytes: {:02x?}", data.len(), &data[..16]);

// 启用日志
RUST_LOG=debug cargo run
RUST_LOG=trace cargo run
RUST_LOG=ech_workers_rs::transport=trace cargo run
```
