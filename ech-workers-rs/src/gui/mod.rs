//! GUI 模块
//! 
//! 基于 egui 的图形界面

pub mod app;
pub mod state;
pub mod panels;
pub mod config;
pub mod service;
pub mod tray;

pub use app::EchWorkersApp;
pub use state::{AppState, ProxyStatus, Statistics};
pub use config::GuiConfig;
pub use service::ProxyService;
