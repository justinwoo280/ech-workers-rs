//! 应用状态管理

use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::RwLock;
use chrono::{DateTime, Local};

/// 代理运行状态
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyStatus {
    Stopped,
    Starting,
    Running,
    Stopping,
    Error,
}

impl ProxyStatus {
    pub fn is_running(&self) -> bool {
        matches!(self, ProxyStatus::Running)
    }

    pub fn to_string(&self) -> &'static str {
        match self {
            ProxyStatus::Stopped => "已停止",
            ProxyStatus::Starting => "启动中...",
            ProxyStatus::Running => "运行中",
            ProxyStatus::Stopping => "停止中...",
            ProxyStatus::Error => "错误",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            ProxyStatus::Stopped => egui::Color32::GRAY,
            ProxyStatus::Starting => egui::Color32::YELLOW,
            ProxyStatus::Running => egui::Color32::GREEN,
            ProxyStatus::Stopping => egui::Color32::YELLOW,
            ProxyStatus::Error => egui::Color32::RED,
        }
    }
}

/// 流量统计
#[derive(Debug, Clone, Default)]
pub struct Statistics {
    /// 上传字节数
    pub upload_bytes: u64,
    /// 下载字节数
    pub download_bytes: u64,
    /// 活跃连接数
    pub active_connections: usize,
    /// 总连接数
    pub total_connections: u64,
    /// 启动时间
    pub start_time: Option<Instant>,
}

impl Statistics {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }

    pub fn uptime(&self) -> Duration {
        self.start_time
            .map(|t| t.elapsed())
            .unwrap_or(Duration::ZERO)
    }

    pub fn format_bytes(bytes: u64) -> String {
        const KB: u64 = 1024;
        const MB: u64 = KB * 1024;
        const GB: u64 = MB * 1024;

        if bytes >= GB {
            format!("{:.2} GB", bytes as f64 / GB as f64)
        } else if bytes >= MB {
            format!("{:.2} MB", bytes as f64 / MB as f64)
        } else if bytes >= KB {
            format!("{:.2} KB", bytes as f64 / KB as f64)
        } else {
            format!("{} B", bytes)
        }
    }

    pub fn format_uptime(duration: Duration) -> String {
        let secs = duration.as_secs();
        let hours = secs / 3600;
        let minutes = (secs % 3600) / 60;
        let seconds = secs % 60;

        if hours > 0 {
            format!("{:02}:{:02}:{:02}", hours, minutes, seconds)
        } else {
            format!("{:02}:{:02}", minutes, seconds)
        }
    }
}

/// 日志条目
#[derive(Debug, Clone)]
pub struct LogEntry {
    pub timestamp: DateTime<Local>,
    pub level: LogLevel,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogLevel {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl LogLevel {
    pub fn to_string(&self) -> &'static str {
        match self {
            LogLevel::Trace => "TRACE",
            LogLevel::Debug => "DEBUG",
            LogLevel::Info => "INFO",
            LogLevel::Warn => "WARN",
            LogLevel::Error => "ERROR",
        }
    }

    pub fn color(&self) -> egui::Color32 {
        match self {
            LogLevel::Trace => egui::Color32::DARK_GRAY,
            LogLevel::Debug => egui::Color32::LIGHT_BLUE,
            LogLevel::Info => egui::Color32::WHITE,
            LogLevel::Warn => egui::Color32::YELLOW,
            LogLevel::Error => egui::Color32::RED,
        }
    }
}

/// 应用状态
pub struct AppState {
    /// 代理状态
    pub status: ProxyStatus,
    /// 统计信息
    pub statistics: Statistics,
    /// 日志缓冲区（最多保留 1000 条）
    pub logs: Vec<LogEntry>,
    /// 最后错误信息
    pub last_error: Option<String>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            status: ProxyStatus::Stopped,
            statistics: Statistics::new(),
            logs: Vec::new(),
            last_error: None,
        }
    }

    pub fn add_log(&mut self, level: LogLevel, message: String) {
        self.logs.push(LogEntry {
            timestamp: Local::now(),
            level,
            message,
        });

        // 限制日志数量
        if self.logs.len() > 1000 {
            self.logs.drain(0..100);
        }
    }

    pub fn clear_logs(&mut self) {
        self.logs.clear();
    }

    pub fn set_status(&mut self, status: ProxyStatus) {
        self.status = status;
        
        match status {
            ProxyStatus::Running => {
                if self.statistics.start_time.is_none() {
                    self.statistics.start_time = Some(Instant::now());
                }
            }
            ProxyStatus::Stopped => {
                self.statistics.reset();
            }
            _ => {}
        }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

/// 线程安全的应用状态
pub type SharedAppState = Arc<RwLock<AppState>>;
