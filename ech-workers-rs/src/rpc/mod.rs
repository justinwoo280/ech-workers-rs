//! JSON-RPC interface for GUI communication via stdin/stdout

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::sync::{mpsc, RwLock};
use tracing::{debug, error, info};

use crate::config::Config;
use crate::error::Result;
use crate::proxy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcRequest {
    pub id: Option<u64>,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum RpcResponse {
    Result {
        id: u64,
        result: serde_json::Value,
    },
    Event {
        event: String,
        data: serde_json::Value,
    },
    Error {
        id: u64,
        error: RpcError,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcError {
    pub code: i32,
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsData {
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub active_connections: usize,
    pub total_connections: u64,
}

/// 代理运行状态
struct ProxyState {
    running: AtomicBool,
    start_time: RwLock<Option<Instant>>,
    shutdown_tx: RwLock<Option<tokio::sync::oneshot::Sender<()>>>,
}

impl ProxyState {
    fn new() -> Self {
        Self {
            running: AtomicBool::new(false),
            start_time: RwLock::new(None),
            shutdown_tx: RwLock::new(None),
        }
    }
}

pub struct RpcServer {
    tx: mpsc::UnboundedSender<RpcResponse>,
    state: Arc<ProxyState>,
}

impl RpcServer {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<RpcResponse>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { 
            tx,
            state: Arc::new(ProxyState::new()),
        }, rx)
    }

    pub async fn run() -> Result<()> {
        let (server, mut rx) = Self::new();
        let server = Arc::new(server);

        let stdout_task = tokio::spawn(async move {
            use tokio::io::AsyncWriteExt;
            let mut stdout = tokio::io::stdout();
            while let Some(response) = rx.recv().await {
                if let Ok(json) = serde_json::to_string(&response) {
                    if let Err(e) = stdout.write_all(json.as_bytes()).await {
                        error!("Failed to write to stdout: {}", e);
                        break;
                    }
                    if let Err(e) = stdout.write_all(b"\n").await {
                        error!("Failed to write newline: {}", e);
                        break;
                    }
                    if let Err(e) = stdout.flush().await {
                        error!("Failed to flush stdout: {}", e);
                        break;
                    }
                }
            }
        });

        let server_clone = Arc::clone(&server);
        let stdin_task = tokio::spawn(async move {
            server_clone.handle_stdin().await
        });

        tokio::select! {
            _ = stdout_task => {},
            _ = stdin_task => {},
        }

        Ok(())
    }

    async fn handle_stdin(self: &Arc<Self>) -> Result<()> {
        let stdin = tokio::io::stdin();
        let mut reader = BufReader::new(stdin);
        let mut line = String::new();

        loop {
            line.clear();
            match reader.read_line(&mut line).await {
                Ok(0) => {
                    debug!("stdin closed");
                    break;
                }
                Ok(_) => {
                    if let Err(e) = self.handle_request(&line).await {
                        error!("Failed to handle request: {}", e);
                    }
                }
                Err(e) => {
                    error!("Failed to read from stdin: {}", e);
                    break;
                }
            }
        }

        Ok(())
    }

    async fn handle_request(self: &Arc<Self>, line: &str) -> Result<()> {
        let request: RpcRequest = serde_json::from_str(line.trim())?;
        debug!("Received RPC request: {:?}", request);

        let response = match request.method.as_str() {
            "start" => self.handle_start(request.id, request.params).await,
            "stop" => self.handle_stop(request.id).await,
            "get_status" => self.handle_get_status(request.id).await,
            "get_config" => self.handle_get_config(request.id).await,
            "save_config" => self.handle_save_config(request.id, request.params).await,
            _ => RpcResponse::Error {
                id: request.id.unwrap_or(0),
                error: RpcError {
                    code: -32601,
                    message: format!("Method not found: {}", request.method),
                },
            },
        };

        self.send_response(response)?;
        Ok(())
    }

    async fn handle_start(self: &Arc<Self>, id: Option<u64>, params: serde_json::Value) -> RpcResponse {
        // 检查是否已经在运行
        if self.state.running.load(Ordering::SeqCst) {
            return RpcResponse::Error {
                id: id.unwrap_or(0),
                error: RpcError {
                    code: -1,
                    message: "Proxy is already running".to_string(),
                },
            };
        }

        // 解析配置
        let config = match Self::parse_config(params) {
            Ok(c) => c,
            Err(e) => {
                return RpcResponse::Error {
                    id: id.unwrap_or(0),
                    error: RpcError {
                        code: -2,
                        message: format!("Invalid config: {}", e),
                    },
                };
            }
        };

        // 创建关闭通道
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        *self.state.shutdown_tx.write().await = Some(shutdown_tx);
        
        // 标记为运行中
        self.state.running.store(true, Ordering::SeqCst);
        *self.state.start_time.write().await = Some(Instant::now());

        // 发送状态更新
        let _ = self.send_status("starting", 0);

        // 启动代理服务
        let state = Arc::clone(&self.state);
        let tx = self.tx.clone();
        tokio::spawn(async move {
            info!("Starting proxy server via RPC...");
            
            // 发送运行状态
            let _ = tx.send(RpcResponse::Event {
                event: "status".to_string(),
                data: serde_json::json!({"status": "running", "uptime_secs": 0}),
            });

            tokio::select! {
                result = proxy::run_server(config) => {
                    if let Err(e) = result {
                        error!("Proxy server error: {}", e);
                        let _ = tx.send(RpcResponse::Event {
                            event: "log".to_string(),
                            data: serde_json::json!({
                                "level": "ERROR",
                                "message": format!("Proxy error: {}", e),
                                "timestamp": chrono::Local::now().to_rfc3339(),
                            }),
                        });
                    }
                }
                _ = shutdown_rx => {
                    info!("Proxy shutdown requested");
                }
            }

            state.running.store(false, Ordering::SeqCst);
            *state.start_time.write().await = None;
            
            let _ = tx.send(RpcResponse::Event {
                event: "status".to_string(),
                data: serde_json::json!({"status": "stopped", "uptime_secs": 0}),
            });
        });

        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({"status": "starting"}),
        }
    }

    fn parse_config(params: serde_json::Value) -> std::result::Result<Config, String> {
        let obj = params.as_object().ok_or("params must be object")?;
        
        Ok(Config {
            listen_addr: obj.get("listen_addr")
                .and_then(|v| v.as_str())
                .unwrap_or("127.0.0.1:1080")
                .to_string(),
            server_addr: obj.get("server_addr")
                .and_then(|v| v.as_str())
                .ok_or("server_addr is required")?
                .to_string(),
            server_ip: obj.get("server_ip")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string()),
            token: obj.get("token")
                .and_then(|v| v.as_str())
                .ok_or("token is required")?
                .to_string(),
            use_ech: obj.get("use_ech")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            ech_domain: obj.get("ech_domain")
                .and_then(|v| v.as_str())
                .unwrap_or("cloudflare-ech.com")
                .to_string(),
            doh_server: obj.get("doh_server")
                .and_then(|v| v.as_str())
                .unwrap_or("223.5.5.5/dns-query")
                .to_string(),
            use_yamux: obj.get("use_yamux")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
            randomize_fingerprint: obj.get("randomize_fingerprint")
                .and_then(|v| v.as_bool())
                .unwrap_or(true),
        })
    }

    async fn handle_stop(&self, id: Option<u64>) -> RpcResponse {
        if !self.state.running.load(Ordering::SeqCst) {
            return RpcResponse::Result {
                id: id.unwrap_or(0),
                result: serde_json::json!({"status": "already_stopped"}),
            };
        }

        // 发送关闭信号
        if let Some(tx) = self.state.shutdown_tx.write().await.take() {
            let _ = tx.send(());
        }

        let _ = self.send_status("stopping", 0);

        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({"status": "stopping"}),
        }
    }

    async fn handle_get_status(&self, id: Option<u64>) -> RpcResponse {
        let running = self.state.running.load(Ordering::SeqCst);
        let uptime = if let Some(start) = *self.state.start_time.read().await {
            start.elapsed().as_secs()
        } else {
            0
        };

        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({
                "status": if running { "running" } else { "stopped" },
                "uptime_secs": uptime
            }),
        }
    }

    async fn handle_get_config(&self, id: Option<u64>) -> RpcResponse {
        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({}),
        }
    }

    async fn handle_save_config(&self, id: Option<u64>, _params: serde_json::Value) -> RpcResponse {
        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({"status": "saved"}),
        }
    }

    fn send_response(&self, response: RpcResponse) -> Result<()> {
        self.tx.send(response)?;
        Ok(())
    }

    pub fn send_event(&self, event: &str, data: serde_json::Value) -> Result<()> {
        self.send_response(RpcResponse::Event {
            event: event.to_string(),
            data,
        })
    }

    #[allow(dead_code)]
    pub fn send_log(&self, level: &str, message: String) -> Result<()> {
        self.send_event(
            "log",
            serde_json::json!({
                "level": level,
                "message": message,
                "timestamp": chrono::Local::now().to_rfc3339(),
            }),
        )
    }

    #[allow(dead_code)]
    pub fn send_status(&self, status: &str, uptime_secs: u64) -> Result<()> {
        self.send_event(
            "status",
            serde_json::json!({
                "status": status,
                "uptime_secs": uptime_secs,
            }),
        )
    }

    #[allow(dead_code)]
    pub fn send_stats(&self, stats: StatsData) -> Result<()> {
        self.send_event("stats", serde_json::to_value(stats)?)
    }
}
