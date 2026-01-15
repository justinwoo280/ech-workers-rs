//! 后端服务集成层
//! 
//! 负责启动/停止代理服务，并与 GUI 状态同步

use std::sync::Arc;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tracing::info;

use crate::config::Config;
use crate::error::Result;
use crate::proxy::run_server;
use super::state::{SharedAppState, ProxyStatus, LogLevel};
use super::config::GuiConfig;

/// 代理服务管理器
pub struct ProxyService {
    /// 应用状态
    state: SharedAppState,
    
    /// 当前运行的服务任务
    task: Arc<RwLock<Option<JoinHandle<()>>>>,
}

impl ProxyService {
    pub fn new(state: SharedAppState) -> Self {
        Self {
            state,
            task: Arc::new(RwLock::new(None)),
        }
    }
    
    /// 启动代理服务
    pub async fn start(&self, gui_config: &GuiConfig) -> Result<()> {
        // 检查是否已经在运行
        {
            let state = self.state.read().await;
            if state.status.is_running() {
                return Ok(());
            }
        }
        
        // 设置状态为启动中
        {
            let mut state = self.state.write().await;
            state.set_status(ProxyStatus::Starting);
            state.add_log(LogLevel::Info, "正在启动代理服务...".to_string());
        }
        
        // 构建配置
        let config = self.build_config(gui_config)?;
        
        // 启动服务
        let state_clone = self.state.clone();
        let task = if gui_config.basic.enable_tun {
            // TUN 模式
            tokio::spawn(async move {
                if let Err(e) = Self::run_tun_mode(config, state_clone.clone()).await {
                    let mut state = state_clone.write().await;
                    state.set_status(ProxyStatus::Error);
                    state.last_error = Some(format!("TUN 模式错误: {}", e));
                    state.add_log(LogLevel::Error, format!("TUN 模式错误: {}", e));
                }
            })
        } else {
            // SOCKS5/HTTP 代理模式
            tokio::spawn(async move {
                if let Err(e) = Self::run_proxy_mode(config, state_clone.clone()).await {
                    let mut state = state_clone.write().await;
                    state.set_status(ProxyStatus::Error);
                    state.last_error = Some(format!("代理服务错误: {}", e));
                    state.add_log(LogLevel::Error, format!("代理服务错误: {}", e));
                }
            })
        };
        
        // 保存任务句柄
        *self.task.write().await = Some(task);
        
        // 设置状态为运行中
        {
            let mut state = self.state.write().await;
            state.set_status(ProxyStatus::Running);
            state.add_log(LogLevel::Info, "代理服务已启动".to_string());
        }
        
        Ok(())
    }
    
    /// 停止代理服务
    pub async fn stop(&self) -> Result<()> {
        // 设置状态为停止中
        {
            let mut state = self.state.write().await;
            state.set_status(ProxyStatus::Stopping);
            state.add_log(LogLevel::Info, "正在停止代理服务...".to_string());
        }
        
        // 取消任务
        if let Some(task) = self.task.write().await.take() {
            task.abort();
        }
        
        // 设置状态为已停止
        {
            let mut state = self.state.write().await;
            state.set_status(ProxyStatus::Stopped);
            state.add_log(LogLevel::Info, "代理服务已停止".to_string());
        }
        
        Ok(())
    }
    
    /// 构建后端配置
    fn build_config(&self, gui_config: &GuiConfig) -> Result<Config> {
        let config = Config {
            listen_addr: gui_config.basic.listen_addr.clone(),
            server_addr: gui_config.basic.server_addr.clone(),
            server_ip: None,
            token: gui_config.basic.token.clone(),
            use_ech: gui_config.ech.enabled,
            ech_domain: gui_config.ech.domain.clone(),
            doh_server: gui_config.ech.doh_server.clone(),
            use_yamux: gui_config.advanced.enable_yamux,
            randomize_fingerprint: gui_config.advanced.enable_fingerprint_randomization,
        };
        
        Ok(config)
    }
    
    /// 运行 SOCKS5/HTTP 代理模式
    async fn run_proxy_mode(config: Config, _state: SharedAppState) -> Result<()> {
        info!("Starting proxy server on {}", config.listen_addr);
        
        // 启动代理服务器
        run_server(config).await
    }
    
    /// 运行 TUN 模式
    async fn run_tun_mode(_config: Config, state: SharedAppState) -> Result<()> {
        info!("Starting TUN mode");
        
        // TODO: 实现 TUN 模式启动
        // 这需要从 main.rs 中的 TUN 启动逻辑迁移过来
        
        let mut state_guard = state.write().await;
        state_guard.add_log(LogLevel::Warn, "TUN 模式暂未完全集成".to_string());
        
        Ok(())
    }
    
    /// 更新统计信息（由后端服务调用）
    pub async fn update_statistics(&self, upload: u64, download: u64, connections: usize) {
        let mut state = self.state.write().await;
        state.statistics.upload_bytes += upload;
        state.statistics.download_bytes += download;
        state.statistics.active_connections = connections;
        state.statistics.total_connections += 1;
    }
}
