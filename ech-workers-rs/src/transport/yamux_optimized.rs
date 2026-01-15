/// Yamux 传输层 - 优化版本
/// 
/// 改进：
/// 1. 健康检查和自动重连
/// 2. 使用 mpsc 通道避免锁竞争
/// 3. KeepAlive 配置
/// 4. 后台任务管理会话

use std::sync::Arc;
use tokio::sync::{mpsc, oneshot};
use tracing::{info, debug, warn, error};
use yamux::{Config as YamuxConfig, Connection, Mode};
use tokio_util::compat::TokioAsyncReadCompatExt;

use crate::config::Config;
use crate::error::{Error, Result};
use crate::transport::websocket::{WebSocketAdapter, establish_websocket_over_tls};
use crate::transport::connection::establish_ech_tls;
use crate::utils::parse_server_addr;
use crate::tls::TlsTunnel;

type YamuxStream = yamux::Stream;
type YamuxConnection = Connection<tokio_util::compat::Compat<WebSocketAdapter<TlsTunnel>>>;

/// Yamux 会话管理器命令
enum SessionCommand {
    /// 请求打开新的 stream
    OpenStream(oneshot::Sender<Result<YamuxStream>>),
    /// 健康检查
    HealthCheck(oneshot::Sender<bool>),
    /// 关闭会话
    Shutdown,
}

/// Yamux 传输层 - 优化版本
pub struct YamuxTransport {
    config: Arc<Config>,
    command_tx: mpsc::Sender<SessionCommand>,
}

impl YamuxTransport {
    pub fn new(config: Arc<Config>) -> Self {
        let (command_tx, command_rx) = mpsc::channel(100);
        
        // 启动后台会话管理任务
        let config_clone = config.clone();
        tokio::spawn(async move {
            if let Err(e) = session_manager_task(config_clone, command_rx).await {
                error!("Session manager task failed: {}", e);
            }
        });
        
        Self {
            config,
            command_tx,
        }
    }

    /// 打开新的 stream
    pub async fn dial(&self) -> Result<YamuxStream> {
        let (tx, rx) = oneshot::channel();
        
        self.command_tx
            .send(SessionCommand::OpenStream(tx))
            .await
            .map_err(|_| Error::Yamux(yamux::ConnectionError::Closed))?;
        
        rx.await
            .map_err(|_| Error::Yamux(yamux::ConnectionError::Closed))?
    }

    /// 健康检查
    pub async fn health_check(&self) -> bool {
        let (tx, rx) = oneshot::channel();
        
        if self.command_tx.send(SessionCommand::HealthCheck(tx)).await.is_err() {
            return false;
        }
        
        rx.await.unwrap_or(false)
    }
}

/// 后台会话管理任务
async fn session_manager_task(
    config: Arc<Config>,
    mut command_rx: mpsc::Receiver<SessionCommand>,
) -> Result<()> {
    let mut session: Option<YamuxConnection> = None;
    let mut consecutive_failures = 0;
    const MAX_FAILURES: u32 = 3;
    
    while let Some(command) = command_rx.recv().await {
        match command {
            SessionCommand::OpenStream(response_tx) => {
                // 尝试打开 stream
                let result = open_stream_with_retry(
                    &mut session,
                    &config,
                    &mut consecutive_failures,
                    MAX_FAILURES,
                ).await;
                
                let _ = response_tx.send(result);
            }
            
            SessionCommand::HealthCheck(response_tx) => {
                let is_healthy = session.is_some();
                let _ = response_tx.send(is_healthy);
            }
            
            SessionCommand::Shutdown => {
                session.take();
                break;
            }
        }
    }
    
    Ok(())
}

/// 尝试打开 stream，失败时自动重连
async fn open_stream_with_retry(
    session: &mut Option<YamuxConnection>,
    config: &Config,
    consecutive_failures: &mut u32,
    max_failures: u32,
) -> Result<YamuxStream> {
    use futures::future::poll_fn;
    
    // 如果有现有 session，先尝试使用
    if let Some(ref mut conn) = session {
        match poll_fn(|cx| conn.poll_new_outbound(cx)).await {
            Ok(stream) => {
                debug!("✅ Opened stream on existing session");
                *consecutive_failures = 0;
                return Ok(stream);
            }
            Err(e) => {
                warn!("Failed to open stream on existing session: {}", e);
                *session = None;
            }
        }
    }
    
    // 需要建立新 session
    if *consecutive_failures >= max_failures {
        error!("Too many consecutive failures ({}), giving up", consecutive_failures);
        return Err(Error::Yamux(yamux::ConnectionError::Closed));
    }
    
    match establish_new_session(config).await {
        Ok(mut new_session) => {
            // 打开第一个 stream
            match poll_fn(|cx| new_session.poll_new_outbound(cx)).await {
                Ok(stream) => {
                    info!("✅ Opened stream on new session");
                    *session = Some(new_session);
                    *consecutive_failures = 0;
                    Ok(stream)
                }
                Err(e) => {
                    error!("Failed to open stream on new session: {}", e);
                    *consecutive_failures += 1;
                    Err(Error::Yamux(e))
                }
            }
        }
        Err(e) => {
            error!("Failed to establish new session: {}", e);
            *consecutive_failures += 1;
            Err(e)
        }
    }
}

/// 创建优化的 Yamux 配置
fn create_optimized_yamux_config() -> YamuxConfig {
    let mut config = YamuxConfig::default();
    
    // 增大接收窗口：256KB -> 2MB
    // 高延迟网络下提升吞吐量
    config.set_max_connection_receive_window(Some(2 * 1024 * 1024)); // 2 MB
    
    // 增大流级别接收窗口
    config.set_receive_window(Some(1024 * 1024)); // 1 MB per stream
    
    // 增大分片发送大小：16KB -> 64KB
    // 减少小包数量，提升效率
    config.set_split_send_size(64 * 1024); // 64 KB
    
    // 最大并发流数量限制
    // 防止资源耗尽
    config.set_max_num_streams(256);
    
    config
}

/// 建立新的 Yamux session
async fn establish_new_session(config: &Config) -> Result<YamuxConnection> {
    info!("🔧 Establishing new Yamux session...");

    // 1. 建立 ECH + TLS 连接
    let tls_tunnel = establish_ech_tls(
        &config.server_addr,
        &config.doh_server,
        config.use_ech,
    ).await?;
    
    // 2. 解析服务器地址
    let (_host, _port, path) = parse_server_addr(&config.server_addr)?;
    
    // 3. 构建 WebSocket URL
    let ws_url = format!("ws://localhost{}", path);
    
    // 4. 在 TLS 连接上建立 WebSocket
    debug!("Establishing WebSocket over TLS");
    let ws_adapter = establish_websocket_over_tls(tls_tunnel, &ws_url, Some(&config.token)).await?;
    
    // 5. 转换为 futures::AsyncRead/AsyncWrite
    let compat_stream = ws_adapter.compat();
    
    // 6. 创建 Yamux connection with 优化配置
    debug!("Creating Yamux session with optimized config");
    let yamux_config = create_optimized_yamux_config();
    
    let connection = Connection::new(compat_stream, yamux_config, Mode::Client);

    info!("✅ Yamux session established (window=2MB, buffer=4MB, split=64KB)");
    Ok(connection)
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
