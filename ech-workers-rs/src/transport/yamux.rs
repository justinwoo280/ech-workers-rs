/// Yamux 传输层
/// 
/// 在 WebSocket 之上建立 Yamux 多路复用

use std::sync::Arc;
use tokio::sync::Mutex as TokioMutex;
use tracing::{info, debug, warn};
use yamux::{Config as YamuxConfig, Connection, Mode};
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::transport::websocket::{WebSocketAdapter, establish_websocket_over_tls};
use crate::transport::connection::establish_ech_tls;
use crate::utils::parse_server_addr;
use crate::tls::TlsTunnel;

/// Yamux 传输层
pub struct YamuxTransport {
    config: Arc<Config>,
    session: Arc<TokioMutex<Option<Connection<tokio_util::compat::Compat<WebSocketAdapter<TlsTunnel>>>>>>,
}

impl YamuxTransport {
    pub fn new(config: Arc<Config>) -> Self {
        Self {
            config,
            session: Arc::new(TokioMutex::new(None)),
        }
    }

    /// 建立新的 Yamux session
    async fn establish_session(&self) -> Result<Connection<tokio_util::compat::Compat<WebSocketAdapter<TlsTunnel>>>> {
        info!("🔧 Establishing new Yamux session...");

        // 1. 建立 ECH + TLS 连接
        let tls_tunnel = establish_ech_tls(
            &self.config.server_addr,
            &self.config.doh_server,
            self.config.use_ech,
        ).await?;
        
        // 2. 解析服务器地址
        let (_host, _port, path) = parse_server_addr(&self.config.server_addr)?;
        
        // 3. 构建 WebSocket URL (使用 ws:// 因为 TLS 已经建立)
        let ws_url = format!("ws://localhost{}", path);
        
        // 4. 在 TLS 连接上建立 WebSocket
        debug!("Establishing WebSocket over TLS");
        let ws_adapter = establish_websocket_over_tls(tls_tunnel, &ws_url, Some(&self.config.token)).await?;
        
        // 5. 转换为 futures::AsyncRead/AsyncWrite (yamux 需要)
        let compat_stream = ws_adapter.compat();
        
        // 6. 创建 Yamux connection with 优化配置
        debug!("Creating Yamux session with optimized config");
        let yamux_config = create_optimized_config();
        let connection = Connection::new(compat_stream, yamux_config, Mode::Client);

        info!("✅ Yamux session established (window=2MB, buffer=4MB)");
        Ok(connection)
    }

    /// 打开新的 stream
    pub async fn dial(&self) -> Result<yamux::Stream> {
        use futures::future::poll_fn;
        
        let mut session_guard = self.session.lock().await;

        // 检查现有 session 是否可用
        if let Some(ref mut conn) = *session_guard {
            match poll_fn(|cx| conn.poll_new_outbound(cx)).await {
                Ok(stream) => {
                    debug!("✅ Opened new stream on existing session");
                    return Ok(stream);
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
        Ok(stream)
    }
}

/// 创建优化的 Yamux 配置
fn create_optimized_config() -> YamuxConfig {
    let mut config = YamuxConfig::default();
    
    // 增大接收窗口：256KB -> 2MB
    config.set_receive_window(2 * 1024 * 1024);
    
    // 增大最大缓冲区：1MB -> 4MB
    config.set_max_buffer_size(4 * 1024 * 1024);
    
    // 增大分片发送大小：16KB -> 64KB
    config.set_split_send_size(64 * 1024);
    
    // 最大并发流数量
    config.set_max_num_streams(256);
    
    config
}

/// WebSocket 传输层（不使用 Yamux）
pub struct WebSocketTransport {
    config: Arc<Config>,
}

impl WebSocketTransport {
    pub fn new(config: Arc<Config>) -> Self {
        Self { config }
    }

    /// 建立 WebSocket 连接
    pub async fn dial(&self) -> Result<WebSocketAdapter<TlsTunnel>> {
        // 1. 建立 ECH + TLS 连接
        let tls_tunnel = establish_ech_tls(
            &self.config.server_addr,
            &self.config.doh_server,
            self.config.use_ech,
        ).await?;
        
        // 2. 解析路径
        let (_host, _port, path) = parse_server_addr(&self.config.server_addr)?;
        let ws_url = format!("ws://localhost{}", path);
        
        // 3. 在 TLS 上建立 WebSocket
        debug!("Establishing WebSocket over TLS");
        establish_websocket_over_tls(tls_tunnel, &ws_url, Some(&self.config.token)).await
    }
}
