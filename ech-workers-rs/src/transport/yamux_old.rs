/// Yamux 传输层
/// 
/// 在 WebSocket 之上建立 Yamux 多路复用
/// 
/// ⚠️ 关键：使用 Box<dyn Io> 隐藏所有底层类型

use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tokio::net::TcpStream;
use tracing::{info, debug, warn};
use yamux::{Config as YamuxConfig, Connection, Mode};
use tokio_util::compat::{TokioAsyncReadCompatExt, Compat};

use crate::config::Config;
use crate::error::{Error, Result};
use crate::stream::{ConnectionContext, Io};
use crate::transport::{tls, websocket};
use crate::utils::parse_server_addr;

/// Yamux 传输层
/// 
/// 维护一个全局 Yamux session，每次 dial 打开新的 stream
/// 
/// ⚠️ 类型简化：Connection<Box<dyn Io>> 而不是具体类型
pub struct YamuxTransport {
    config: Arc<Config>,
    session: Arc<TokioMutex<Option<Connection<Box<dyn Io>>>>>,
}

impl YamuxTransport {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            session: Arc::new(TokioMutex::new(None)),
        }
    }

    /// 建立新的 Yamux session
    /// 
    /// 返回 Connection<Box<dyn Io>>，隐藏所有底层类型
    async fn establish_session(&self) -> Result<Connection<Box<dyn Io>>> {
        info!("🔧 Establishing new Yamux session...");

        // 1. 解析服务器地址
        let (host, port, path) = parse_server_addr(&self.config.server_addr)?;
        let addr = format!("{}:{}", host, port);

        // 2. 建立 TCP 连接
        debug!("Connecting to TCP {}", addr);
        let tcp = if let Some(ref server_ip) = self.config.server_ip {
            // 使用指定的 IP
            let ip_addr = format!("{}:{}", server_ip, port);
            TcpStream::connect(&ip_addr).await
                .map_err(|e| Error::Io(e))?
        } else {
            TcpStream::connect(&addr).await
                .map_err(|e| Error::Io(e))?
        };

        // 3. 建立 TLS 连接
        debug!("Establishing TLS connection");
        let tls_stream = tls::establish_tls(tcp, &host).await?;
        

        // 4. 建立 WebSocket 连接
        debug!("Establishing WebSocket connection");
        let ws_url = format!("wss://{}:{}{}", host, port, path);
        let ws_io: Box<dyn Io> = websocket::establish_websocket(
            tls_stream,
            &ws_url,
            Some(&self.config.token),
        ).await?;

        // 5. 建立 Yamux session
        // ⚠️ Connection 只关心 AsyncRead + AsyncWrite + Unpin
        // Box<dyn Io> 完全满足要求
        debug!("Creating Yamux session");
        let yamux_config = YamuxConfig::default();
        let connection = Connection::new(ws_io, yamux_config, Mode::Client);

        info!("✅ Yamux session established");
        Ok(connection)
    }

    /// 获取或创建 session，并打开新的 stream
    /// 
    /// 返回 Box<dyn Io>，隐藏 YamuxStream 类型
    async fn open_stream(&self) -> Result<Box<dyn Io>> {
        use futures::future::poll_fn;
        use tokio_util::compat::FuturesAsyncReadCompatExt;
        
        let mut session_guard = self.session.lock().await;

        // 检查现有 session 是否可用
        if let Some(ref mut conn) = *session_guard {
            match poll_fn(|cx| conn.poll_new_outbound(cx)).await {
                Ok(stream) => {
                    debug!("✅ Opened new stream on existing session");
                    // 转换 futures::AsyncRead/Write 到 tokio::AsyncRead/Write
                    let compat_stream = stream.compat();
                    return Ok(Box::new(compat_stream) as Box<dyn Io>);
                }
                Err(e) => {
                    warn!("Failed to open stream on existing session: {}, creating new session", e);
                    *session_guard = None;
                }
            }
        }

        // 建立新 session
        let mut new_session = self.establish_session().await?;
        
        // 打开第一个 stream
        let stream = poll_fn(|cx| new_session.poll_new_outbound(cx)).await
            .map_err(|e| Error::Yamux(e))?;

        // 保存 session
        *session_guard = Some(new_session);

        debug!("✅ Opened stream on new session");
        // 转换并类型擦除
        let compat_stream = stream.compat();
        Ok(Box::new(compat_stream) as Box<dyn Io>)
    }

    /// 直接连接（供外部调用）
    pub async fn dial(&self) -> Result<ConnectionContext> {
        info!("🔌 Dialing via Yamux...");

        // 打开 Yamux stream
        let stream = self.open_stream().await?;

        // 包装为 ConnectionContext
        let ctx = ConnectionContext::new(
            Box::pin(stream),
            self.config.server_addr.clone(),
            true,  // is_secure (TLS)
            self.config.use_ech,
            true,  // is_yamux
        );

        info!("✅ Yamux connection established");
        Ok(ctx)
    }
}

impl YamuxTransport {
    /// 获取传输层名称
    pub fn name(&self) -> &str {
        if self.config.use_ech {
            "Yamux+WebSocket+ECH+TLS1.3"
        } else {
            "Yamux+WebSocket+TLS1.3"
        }
    }
}

/// WebSocket 传输层（简单模式，无 Yamux）
pub struct WebSocketTransport {
    config: Arc<Config>,
}

impl WebSocketTransport {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
        }
    }

    /// 建立连接
    pub async fn dial(&self) -> Result<ConnectionContext> {
        info!("🔌 Establishing WebSocket connection (no Yamux)...");

        // 1. 解析服务器地址
        let (host, port, path) = parse_server_addr(&self.config.server_addr)?;
        let addr = format!("{}:{}", host, port);

        // 2. 建立 TCP 连接
        debug!("Connecting to TCP {}", addr);
        let tcp = if let Some(ref server_ip) = self.config.server_ip {
            let ip_addr = format!("{}:{}", server_ip, port);
            TcpStream::connect(&ip_addr).await?
        } else {
            TcpStream::connect(&addr).await?
        };

        // 3. 建立 TLS 连接
        debug!("Establishing TLS connection");
        let tls_stream = tls::establish_tls(tcp, &host).await?;
        

        // 4. 建立 WebSocket 连接
        debug!("Establishing WebSocket connection");
        let ws_url = format!("wss://{}:{}{}", host, port, path);
        let ws_io: Box<dyn Io> = websocket::establish_websocket(
            tls_stream,
            &ws_url,
            Some(&self.config.token),
        ).await?;

        // 5. 包装为 ConnectionContext
        let ctx = ConnectionContext::new(
            Box::pin(ws_io),
            self.config.server_addr.clone(),
            true,  // is_secure (TLS)
            self.config.use_ech,
            false, // is_yamux
        );

        info!("✅ WebSocket connection established");
        Ok(ctx)
    }

    /// 获取传输层名称
    pub fn name(&self) -> &str {
        if self.config.use_ech {
            "WebSocket+ECH+TLS1.3"
        } else {
            "WebSocket+TLS1.3"
        }
    }
}
