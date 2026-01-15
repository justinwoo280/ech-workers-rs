# GUI 模块

基于 egui 的图形界面，支持代理服务管理和系统托盘。

## 模块结构

```
gui/
├── mod.rs              # 模块导出
├── app.rs              # 主应用程序（EchWorkersApp）
├── state.rs            # 应用状态管理（AppState, ProxyStatus, Statistics）
├── config.rs           # GUI 配置持久化（GuiConfig）
├── service.rs          # 后端服务集成层（ProxyService）
├── tray.rs             # 系统托盘管理（TrayManager）
└── panels/
    ├── mod.rs          # 面板模块
    ├── dashboard.rs    # 状态面板
    ├── settings.rs     # 配置面板
    └── logs.rs         # 日志面板
```

## 功能特性

### 状态面板（Dashboard）
- 实时连接状态显示
- 运行时间统计
- 流量统计（上传/下载）
- 活跃连接数和总连接数

### 配置面板（Settings）
- **基本设置**：监听地址、服务器地址、Token、TUN 模式
- **ECH 设置**：启用/禁用、ECH 域名、DoH 服务器
- **高级设置**：Yamux 多路复用、指纹随机化、TLS 配置
- **应用设置**：开机自启、最小化到托盘

### 日志面板（Logs）
- 实时日志显示
- 日志级别过滤（TRACE/DEBUG/INFO/WARN/ERROR）
- 搜索功能
- 自动滚动
- 清空日志

### 系统托盘
- Windows 系统托盘图标
- 右键菜单（显示窗口、退出）
- 状态提示（运行中/已停止）
- 点击图标显示窗口

## 使用方法

### 启动 GUI

```bash
ech-workers-rs gui
```

### 配置文件

配置自动保存到用户配置目录：
- Windows: `%APPDATA%\ech-workers\ech-workers-rs\config.toml`
- Linux: `~/.config/ech-workers/ech-workers-rs/config.toml`
- macOS: `~/Library/Application Support/com.ech-workers.ech-workers-rs/config.toml`

### 集成后端服务

`ProxyService` 负责启动/停止代理服务：

```rust
// 启动代理
proxy_service.start(&gui_config).await?;

// 停止代理
proxy_service.stop().await?;

// 更新统计信息
proxy_service.update_statistics(upload, download, connections).await;
```

## 技术栈

- **eframe/egui**: 跨平台 GUI 框架
- **tokio**: 异步运行时
- **tray-icon**: 系统托盘支持（Windows）
- **toml**: 配置文件格式
- **directories**: 跨平台配置目录

## 待完成

- [ ] Linux/macOS 系统托盘支持
- [ ] TUN 模式完整集成
- [ ] 流量图表显示
- [ ] 开机自启功能
- [ ] 更新检查
