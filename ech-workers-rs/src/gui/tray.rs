//! 系统托盘功能
//! 
//! 支持 Windows/Linux/macOS 系统托盘图标和菜单

#[cfg(target_os = "windows")]
use tray_icon::{
    TrayIconBuilder, TrayIconEvent, 
    menu::{Menu, MenuItem, PredefinedMenuItem},
};


/// 托盘图标管理器
pub struct TrayManager {
    #[cfg(target_os = "windows")]
    _tray_icon: Option<tray_icon::TrayIcon>,
}

impl TrayManager {
    /// 创建托盘管理器
    pub fn new() -> Self {
        Self {
            #[cfg(target_os = "windows")]
            _tray_icon: None,
        }
    }
    
    /// 初始化系统托盘
    #[cfg(target_os = "windows")]
    pub fn init(&mut self) -> anyhow::Result<()> {
        use tray_icon::Icon;
        
        // 创建托盘菜单
        let tray_menu = Menu::new();
        
        let show_item = MenuItem::new("显示窗口", true, None);
        let separator = PredefinedMenuItem::separator();
        let quit_item = MenuItem::new("退出", true, None);
        
        tray_menu.append(&show_item)?;
        tray_menu.append(&separator)?;
        tray_menu.append(&quit_item)?;
        
        // 创建托盘图标（使用默认图标）
        let icon = Icon::from_rgba(vec![255u8; 32 * 32 * 4], 32, 32)?;
        
        let tray_icon = TrayIconBuilder::new()
            .with_menu(Box::new(tray_menu))
            .with_tooltip("ECH Workers RS")
            .with_icon(icon)
            .build()?;
        
        self._tray_icon = Some(tray_icon);
        
        Ok(())
    }
    
    /// 初始化系统托盘（非 Windows 平台）
    #[cfg(not(target_os = "windows"))]
    pub fn init(&mut self) -> anyhow::Result<()> {
        // TODO: 实现 Linux/macOS 托盘支持
        Ok(())
    }
    
    /// 处理托盘事件
    #[cfg(target_os = "windows")]
    pub fn handle_events(&self) -> Option<TrayEvent> {
        use tray_icon::menu::MenuEvent;
        
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            let _id = event.id.0;
            
            // 根据菜单项 ID 判断事件
            // 这里需要保存菜单项的 ID 引用
            // 简化处理：返回事件类型
            return Some(TrayEvent::MenuClick);
        }
        
        if let Ok(_event) = TrayIconEvent::receiver().try_recv() {
            return Some(TrayEvent::IconClick);
        }
        
        None
    }
    
    /// 处理托盘事件（非 Windows 平台）
    #[cfg(not(target_os = "windows"))]
    pub fn handle_events(&self) -> Option<TrayEvent> {
        None
    }
    
    /// 更新托盘图标状态
    pub fn update_status(&mut self, running: bool) {
        #[cfg(target_os = "windows")]
        {
            if let Some(ref tray) = self._tray_icon {
                let tooltip = if running {
                    "ECH Workers RS - 运行中"
                } else {
                    "ECH Workers RS - 已停止"
                };
                let _ = tray.set_tooltip(Some(tooltip));
            }
        }
        
        #[cfg(not(target_os = "windows"))]
        {
            let _ = running; // 避免未使用警告
        }
    }
}

/// 托盘事件
#[derive(Debug, Clone, Copy)]
pub enum TrayEvent {
    /// 图标点击
    IconClick,
    /// 菜单点击
    MenuClick,
    /// 显示窗口
    Show,
    /// 退出应用
    Quit,
}
