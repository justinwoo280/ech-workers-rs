//! 主应用程序

use std::sync::Arc;
use tokio::sync::RwLock;
use eframe::egui;

use super::state::{AppState, SharedAppState, LogLevel};
use super::config::GuiConfig;
use super::panels::{DashboardPanel, SettingsPanel, LogsPanel};
use super::service::ProxyService;
use super::tray::TrayManager;

/// 主应用
pub struct EchWorkersApp {
    /// 应用状态
    state: SharedAppState,
    
    /// 配置
    config: GuiConfig,
    
    /// 当前选中的标签页
    active_tab: Tab,
    
    /// 日志面板
    logs_panel: LogsPanel,
    
    /// 配置是否已修改
    config_dirty: bool,
    
    /// 代理服务管理器
    proxy_service: Arc<ProxyService>,
    
    /// 系统托盘管理器
    tray_manager: TrayManager,
    
    /// Tokio 运行时
    runtime: tokio::runtime::Runtime,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Tab {
    Dashboard,
    Settings,
    Logs,
}

impl EchWorkersApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> Self {
        println!("[DEBUG] EchWorkersApp::new() called");
        
        // 设置字体
        Self::configure_fonts(&cc.egui_ctx);
        println!("[DEBUG] Fonts configured");
        
        // 设置主题
        cc.egui_ctx.set_visuals(egui::Visuals::dark());
        println!("[DEBUG] Theme set");
        
        // 加载配置
        let config = GuiConfig::load().unwrap_or_default();
        println!("[DEBUG] Config loaded");
        
        // 创建应用状态
        let state = Arc::new(RwLock::new(AppState::new()));
        println!("[DEBUG] State created");
        
        // 添加初始日志
        {
            let mut state_guard = state.blocking_write();
            state_guard.add_log(LogLevel::Info, "ECH Workers RS 已启动".to_string());
        }
        
        // 创建 Tokio 运行时
        let runtime = tokio::runtime::Runtime::new().expect("Failed to create Tokio runtime");
        println!("[DEBUG] Tokio runtime created");
        
        // 创建代理服务管理器
        let proxy_service = Arc::new(ProxyService::new(state.clone()));
        println!("[DEBUG] ProxyService created");
        
        // 创建托盘管理器 (暂时禁用)
        let tray_manager = TrayManager::new();
        // if let Err(e) = tray_manager.init() {
        //     let mut state_guard = state.blocking_write();
        //     state_guard.add_log(LogLevel::Warn, format!("托盘初始化失败: {}", e));
        // }
        println!("[DEBUG] TrayManager created (init disabled)");
        
        println!("[DEBUG] EchWorkersApp::new() completed");
        
        Self {
            state,
            config,
            active_tab: Tab::Dashboard,
            logs_panel: LogsPanel::default(),
            config_dirty: false,
            proxy_service,
            tray_manager,
            runtime,
        }
    }
    
    fn configure_fonts(ctx: &egui::Context) {
        let fonts = egui::FontDefinitions::default();
        
        // 添加中文字体支持（如果需要）
        // 这里使用系统默认字体
        
        ctx.set_fonts(fonts);
    }
    
    fn show_top_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.heading("🚀 ECH Workers RS");
                
                ui.separator();
                
                // 标签页切换
                ui.selectable_value(&mut self.active_tab, Tab::Dashboard, "📊 状态");
                ui.selectable_value(&mut self.active_tab, Tab::Settings, "⚙ 设置");
                ui.selectable_value(&mut self.active_tab, Tab::Logs, "📝 日志");
                
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    // 启动/停止按钮
                    let state = self.state.blocking_read();
                    let is_running = state.status.is_running();
                    drop(state);
                    
                    if is_running {
                        if ui.button("⏹ 停止").clicked() {
                            self.stop_proxy();
                        }
                    } else {
                        if ui.button("▶ 启动").clicked() {
                            self.start_proxy();
                        }
                    }
                });
            });
        });
    }
    
    fn show_bottom_panel(&mut self, ctx: &egui::Context) {
        egui::TopBottomPanel::bottom("bottom_panel").show(ctx, |ui| {
            ui.horizontal(|ui| {
                ui.label("ECH Workers RS v0.1.0");
                ui.separator();
                
                let state = self.state.blocking_read();
                ui.label(format!("状态: {}", state.status.to_string()));
                
                if self.config_dirty {
                    ui.separator();
                    ui.label(egui::RichText::new("⚠ 配置未保存").color(egui::Color32::YELLOW));
                }
            });
        });
    }
    
    fn show_central_panel(&mut self, ctx: &egui::Context) {
        egui::CentralPanel::default().show(ctx, |ui| {
            match self.active_tab {
                Tab::Dashboard => {
                    let state = self.state.blocking_read();
                    DashboardPanel::show(ui, &state);
                }
                Tab::Settings => {
                    let changed = SettingsPanel::show(ui, &mut self.config);
                    if changed {
                        self.config_dirty = true;
                    }
                    
                    ui.add_space(10.0);
                    
                    // 保存按钮
                    if self.config_dirty {
                        ui.horizontal(|ui| {
                            if ui.button("💾 保存配置").clicked() {
                                if let Err(e) = self.config.save() {
                                    let mut state = self.state.blocking_write();
                                    state.add_log(LogLevel::Error, format!("保存配置失败: {}", e));
                                } else {
                                    let mut state = self.state.blocking_write();
                                    state.add_log(LogLevel::Info, "配置已保存".to_string());
                                    self.config_dirty = false;
                                }
                            }
                            
                            if ui.button("↺ 重置").clicked() {
                                self.config = GuiConfig::load().unwrap_or_default();
                                self.config_dirty = false;
                            }
                        });
                    }
                }
                Tab::Logs => {
                    let mut state = self.state.blocking_write();
                    self.logs_panel.show(ui, &mut state);
                }
            }
        });
    }
    
    fn start_proxy(&mut self) {
        let proxy_service = self.proxy_service.clone();
        let config = self.config.clone();
        
        self.runtime.spawn(async move {
            if let Err(e) = proxy_service.start(&config).await {
                tracing::error!("Failed to start proxy: {}", e);
            }
        });
    }
    
    fn stop_proxy(&mut self) {
        let proxy_service = self.proxy_service.clone();
        
        self.runtime.spawn(async move {
            if let Err(e) = proxy_service.stop().await {
                tracing::error!("Failed to stop proxy: {}", e);
            }
        });
    }
}

impl eframe::App for EchWorkersApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        // 处理托盘事件
        if let Some(event) = self.tray_manager.handle_events() {
            use super::tray::TrayEvent;
            match event {
                TrayEvent::IconClick | TrayEvent::Show => {
                    // eframe 0.28: 使用 ViewportCommand 控制窗口
                    ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                    ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                }
                TrayEvent::Quit => {
                    ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                _ => {}
            }
        }
        
        // 更新托盘状态
        let is_running = self.state.blocking_read().status.is_running();
        self.tray_manager.update_status(is_running);
        
        self.show_top_panel(ctx);
        self.show_bottom_panel(ctx);
        self.show_central_panel(ctx);
        
        // 定期刷新（用于更新统计信息）
        ctx.request_repaint_after(std::time::Duration::from_secs(1));
    }
    
    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        // 停止代理服务
        let proxy_service = self.proxy_service.clone();
        self.runtime.block_on(async move {
            let _ = proxy_service.stop().await;
        });
        
        // 保存配置
        if self.config_dirty {
            let _ = self.config.save();
        }
    }
}
