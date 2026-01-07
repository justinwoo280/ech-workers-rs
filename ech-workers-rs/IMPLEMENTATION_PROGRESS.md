# ECH Workers RS 实现进度

## 已完成 ✅

### 1. 核心 ECH 功能
- ✅ DoH (DNS-over-HTTPS) 实现 (`src/ech/doh.rs`)
- ✅ Zig TLS Tunnel 集成 (`src/tls/`)
- ✅ ECH 配置查询和传递
- ✅ ECH 握手验证
- ✅ 端到端测试通过

### 2. 传输层
- ✅ WebSocket 适配器 (`src/transport/websocket.rs`)
  - 将 WebSocketStream 转换为 AsyncRead/AsyncWrite
  - 支持通过已有 TLS 连接建立 WebSocket
- ✅ Yamux 多路复用 (`src/transport/yamux.rs`)
  - 修复 futures/tokio trait 问题
  - 使用 tokio_util::compat 转换
- ✅ 连接建立流程 (`src/transport/connection.rs`)
  - DoH → ECH → Zig TLS → WebSocket → Yamux

### 3. TLS 集成
- ✅ TlsTunnel 实现 AsyncRead/AsyncWrite
- ✅ 支持作为 WebSocket 的底层传输

## 进行中 ⚠️

### 代理功能
- ⚠️ SOCKS5 代理处理器 (`src/proxy/socks5.rs`)
- ⚠️ HTTP CONNECT 代理处理器 (`src/proxy/http.rs`)
- ⚠️ 请求路由和转发 (`src/proxy/handler.rs`)

## 待实现 📋

### 1. 完整的代理流程
```
客户端 (SOCKS5/HTTP)
    ↓
本地代理 (127.0.0.1:1080)
    ↓
DoH 查询 ECH 配置
    ↓
Zig TLS Tunnel (ECH + TLS 1.3)
    ↓
WebSocket
    ↓
Yamux (可选)
    ↓
远程服务器
```

### 2. SOCKS5 实现
需要实现：
- SOCKS5 握手
- 认证（如果需要）
- CONNECT 命令处理
- 数据转发

### 3. HTTP CONNECT 实现
需要实现：
- HTTP CONNECT 请求解析
- 200 Connection Established 响应
- 数据转发

### 4. 服务端兼容性
需要兼容 Go 版本的 proxy-server：
- WebSocket 协议检测
- Yamux 协议支持
- 简单 WebSocket 模式支持

## 架构设计

### 连接流程

#### 客户端 → 服务器
```rust
// 1. 本地代理接收 SOCKS5/HTTP 请求
let local_stream = listener.accept().await?;

// 2. 解析目标地址
let target = parse_socks5_request(&local_stream)?;

// 3. 建立到服务器的连接
let config = Arc::new(Config { ... });

// 选项 A: 使用 Yamux
let yamux_transport = YamuxTransport::new(config);
let remote_stream = yamux_transport.dial().await?;

// 选项 B: 简单 WebSocket
let ws_transport = WebSocketTransport::new(config);
let remote_stream = ws_transport.dial().await?;

// 4. 双向转发
tokio::io::copy_bidirectional(&mut local_stream, &mut remote_stream).await?;
```

#### 连接建立细节
```rust
// YamuxTransport::dial()
async fn dial(&self) -> Result<yamux::Stream> {
    // 1. 建立 ECH + TLS 连接
    let tls_tunnel = establish_ech_tls(
        &self.config.server_addr,
        &self.config.doh_server,
        self.config.use_ech,
    ).await?;
    
    // 2. 在 TLS 上建立 WebSocket
    let ws_adapter = establish_websocket_over_tls(
        tls_tunnel,
        &ws_url,
        Some(&self.config.token)
    ).await?;
    
    // 3. 转换为 futures traits (yamux 需要)
    let compat_stream = ws_adapter.compat();
    
    // 4. 创建或复用 Yamux session
    let mut session = self.session.lock().await;
    if session.is_none() {
        *session = Some(Connection::new(compat_stream, config, Mode::Client));
    }
    
    // 5. 打开新 stream
    session.as_mut().unwrap().open_stream().await
}
```

### 数据流

```
SOCKS5 Client
    ↓ [SOCKS5 Protocol]
Local Proxy (127.0.0.1:1080)
    ↓ [Parse Target]
    ↓ [DoH Query ECH]
    ↓ [Zig TLS + ECH]
    ↓ [WebSocket Frames]
    ↓ [Yamux Streams (optional)]
Remote Server (proxy-server)
    ↓ [Target Connection]
Target Server
```

## 下一步实现

### 优先级 1: 基本代理功能

1. **实现 SOCKS5 处理器**
```rust
// src/proxy/socks5.rs
pub async fn handle_socks5(
    mut local: TcpStream,
    transport: Arc<dyn Transport>,
) -> Result<()> {
    // 1. SOCKS5 握手
    let target = socks5_handshake(&mut local).await?;
    
    // 2. 建立远程连接
    let mut remote = transport.dial().await?;
    
    // 3. 发送目标地址到服务器
    send_target(&mut remote, &target).await?;
    
    // 4. 双向转发
    tokio::io::copy_bidirectional(&mut local, &mut remote).await?;
    
    Ok(())
}
```

2. **实现 HTTP CONNECT 处理器**
```rust
// src/proxy/http.rs
pub async fn handle_http_connect(
    mut local: TcpStream,
    transport: Arc<dyn Transport>,
) -> Result<()> {
    // 1. 解析 CONNECT 请求
    let target = parse_connect_request(&mut local).await?;
    
    // 2. 建立远程连接
    let mut remote = transport.dial().await?;
    
    // 3. 发送 200 响应
    local.write_all(b"HTTP/1.1 200 Connection Established\r\n\r\n").await?;
    
    // 4. 双向转发
    tokio::io::copy_bidirectional(&mut local, &mut remote).await?;
    
    Ok(())
}
```

3. **实现主代理服务器**
```rust
// src/proxy/mod.rs
pub async fn run_server(config: Config) -> Result<()> {
    let listener = TcpListener::bind(&config.listen_addr).await?;
    info!("Listening on {}", config.listen_addr);
    
    let config = Arc::new(config);
    
    loop {
        let (stream, addr) = listener.accept().await?;
        let config = config.clone();
        
        tokio::spawn(async move {
            if let Err(e) = handle_connection(stream, config).await {
                error!("Connection error from {}: {}", addr, e);
            }
        });
    }
}

async fn handle_connection(stream: TcpStream, config: Arc<Config>) -> Result<()> {
    // 检测协议类型
    let mut buf = [0u8; 1];
    stream.peek(&mut buf).await?;
    
    match buf[0] {
        0x05 => handle_socks5(stream, config).await,
        b'C' | b'G' | b'P' => handle_http(stream, config).await,
        _ => Err(Error::Protocol("Unknown protocol".into())),
    }
}
```

### 优先级 2: 服务端协议

实现与 Go proxy-server 兼容的协议：

1. **目标地址传输**
```rust
// 发送目标地址 (SOCKS5 格式)
async fn send_target<W: AsyncWrite + Unpin>(
    writer: &mut W,
    target: &TargetAddr,
) -> Result<()> {
    match target {
        TargetAddr::Ip(addr) => {
            // ATYP + ADDR + PORT
            writer.write_all(&[0x01]).await?; // IPv4
            writer.write_all(&addr.ip().octets()).await?;
            writer.write_u16(addr.port()).await?;
        }
        TargetAddr::Domain(domain, port) => {
            writer.write_all(&[0x03]).await?; // Domain
            writer.write_u8(domain.len() as u8).await?;
            writer.write_all(domain.as_bytes()).await?;
            writer.write_u16(*port).await?;
        }
    }
    Ok(())
}
```

2. **协议检测**
服务端需要检测是 Yamux 还是简单 WebSocket：
- Yamux: 第一个字节是 Yamux 协议头
- 简单模式: 直接是 SOCKS5 地址

### 优先级 3: 测试和优化

1. **端到端测试**
```bash
# 启动服务器 (Go 版本)
cd /workspaces/jarustls/ech-workers/proxy-server
go run main.go -listen :8443 -cert cert.pem -key key.pem

# 启动客户端 (Rust 版本)
cd /workspaces/jarustls/ech-workers-rs
cargo run --release -- \
  --listen 127.0.0.1:1080 \
  --server example.com:8443/ws \
  --token mytoken \
  --ech \
  --yamux

# 测试 SOCKS5
curl --socks5 127.0.0.1:1080 https://www.google.com

# 测试 HTTP CONNECT
curl --proxy 127.0.0.1:1080 https://www.google.com
```

2. **性能优化**
- 连接池
- ECH 配置缓存
- Yamux session 复用

## 当前状态

### 可以编译 ✅
```bash
cd /workspaces/jarustls/ech-workers-rs
cargo check  # 通过
```

### 核心功能可用 ✅
- ECH + TLS 连接
- WebSocket 传输
- Yamux 多路复用

### 需要完成 ⚠️
- SOCKS5/HTTP 代理逻辑
- 与服务端的协议对接
- 完整的数据转发

## 参考

- Go 客户端: `/workspaces/jarustls/ech-workers/ech-workers/`
- Go 服务端: `/workspaces/jarustls/ech-workers/proxy-server/`
- ECH 集成: `ECH_INTEGRATION.md`
- 安全验证: `zig-tls-tunnel/ECH_SECURITY_VERIFICATION.md`
