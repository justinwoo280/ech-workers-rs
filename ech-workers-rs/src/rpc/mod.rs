//! JSON-RPC interface for GUI communication via stdin/stdout

use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::sync::mpsc;
use tracing::{debug, error};

use crate::error::Result;

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
pub struct StatusData {
    pub status: String,
    pub uptime_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatsData {
    pub upload_bytes: u64,
    pub download_bytes: u64,
    pub active_connections: usize,
    pub total_connections: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogData {
    pub level: String,
    pub message: String,
    pub timestamp: String,
}

pub struct RpcServer {
    tx: mpsc::UnboundedSender<RpcResponse>,
}

impl RpcServer {
    pub fn new() -> (Self, mpsc::UnboundedReceiver<RpcResponse>) {
        let (tx, rx) = mpsc::unbounded_channel();
        (Self { tx }, rx)
    }

    pub async fn run() -> Result<()> {
        let (server, mut rx) = Self::new();

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

        let stdin_task = tokio::spawn(async move {
            server.handle_stdin().await
        });

        tokio::select! {
            _ = stdout_task => {},
            _ = stdin_task => {},
        }

        Ok(())
    }

    async fn handle_stdin(&self) -> Result<()> {
        use tokio::io::AsyncBufReadExt;
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

    async fn handle_request(&self, line: &str) -> Result<()> {
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

    async fn handle_start(&self, id: Option<u64>, _params: serde_json::Value) -> RpcResponse {
        // TODO: Parse config and start proxy
        
        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({"status": "starting"}),
        }
    }

    async fn handle_stop(&self, id: Option<u64>) -> RpcResponse {
        // TODO: Stop proxy
        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({"status": "stopped"}),
        }
    }

    async fn handle_get_status(&self, id: Option<u64>) -> RpcResponse {
        // TODO: Get actual status
        RpcResponse::Result {
            id: id.unwrap_or(0),
            result: serde_json::json!({
                "status": "stopped",
                "uptime_secs": 0
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

    pub fn send_status(&self, status: &str, uptime_secs: u64) -> Result<()> {
        self.send_event(
            "status",
            serde_json::json!({
                "status": status,
                "uptime_secs": uptime_secs,
            }),
        )
    }

    pub fn send_stats(&self, stats: StatsData) -> Result<()> {
        self.send_event("stats", serde_json::to_value(stats)?)
    }
}
