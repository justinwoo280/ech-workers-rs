# ECH Workers RS - Qt C++ GUI 实施指南

## 概述

为 `ech-workers-rs` 项目开发的 **Qt C++ GUI**，通过 **stdin/stdout JSON-RPC** 与 Rust 后端通信。

---

## 架构设计

### 组件关系

```
┌─────────────────────────────────────────┐
│  Qt C++ GUI (ech-workers-gui.exe)      │
│  ├─ MainWindow (主窗口)                 │
│  ├─ ProcessManager (进程管理)          │
│  ├─ ConfigManager (配置管理)           │
│  └─ TrayManager (系统托盘)             │
└─────────────┬───────────────────────────┘
              │ QProcess
              │ stdin/stdout (JSON-RPC)
┌─────────────▼───────────────────────────┐
│  Rust Backend (ech-workers-rs.exe)     │
│  启动参数: --json-rpc                   │
│  ├─ 读取 stdin (接收命令)               │
│  └─ 输出 stdout (发送事件/状态)         │
└────────────────────────────────────────┘
```

### JSON-RPC 通信协议

#### GUI → Rust (Command)

```json
{
  "id": 1,
  "method": "start",
  "params": {
    "basic": {
      "listen_addr": "127.0.0.1:1080",
      "server_addr": "worker.example.com",
      "token": "secret-token",
      "enable_tun": false
    },
    "ech": {
      "enabled": true,
      "domain": "cloudflare-ech.com",
      "doh_server": "https://1.1.1.1/dns-query"
    },
    "advanced": {
      "enable_yamux": true,
      "enable_fingerprint_randomization": true,
      "tls_profile": "Chrome"
    }
  }
}
```

```json
{"id": 2, "method": "stop"}
```

```json
{"id": 3, "method": "get_status"}
```

#### Rust → GUI (Response/Event)

**RPC Response**
```json
{"id": 1, "result": {"status": "starting"}}
```

**Status Event**
```json
{
  "event": "status",
  "data": {
    "status": "running",
    "uptime_secs": 120
  }
}
```

**Log Event**
```json
{
  "event": "log",
  "data": {
    "level": "info",
    "message": "代理服务已启动",
    "timestamp": "2026-01-17T11:30:00+08:00"
  }
}
```

**Stats Event**
```json
{
  "event": "stats",
  "data": {
    "upload_bytes": 1048576,
    "download_bytes": 2097152,
    "active_connections": 5,
    "total_connections": 127
  }
}
```

---

## Rust 后端适配

### 1. 添加 RPC 模块

**文件**: `ech-workers-rs/src/rpc/mod.rs`

已创建完整的 RPC 服务器实现，支持：
- stdin 读取 JSON 命令
- stdout 输出 JSON 响应
- 异步事件推送（日志、状态、统计）

### 2. 修改 main.rs

**添加 `--json-rpc` 命令行参数**

```rust
mod rpc;

#[derive(Parser, Debug)]
struct Args {
    #[arg(long)]
    json_rpc: bool,
    
    // ... 其他参数
}

#[tokio::main]
async fn main() -> Result<()> {
    let args = Args::parse();
    
    if args.json_rpc {
        // JSON-RPC 模式
        return rpc::RpcServer::run().await;
    }
    
    // 原有 CLI 模式
    // ...
}
```

### 3. 集成到 GUI Service

修改 `gui/service.rs`，在启动/停止时触发 RPC 事件：

```rust
use crate::rpc::RpcServer;

pub struct ProxyService {
    state: SharedAppState,
    task: Arc<RwLock<Option<JoinHandle<()>>>>,
    rpc_server: Option<Arc<RpcServer>>,
}

impl ProxyService {
    pub async fn start(&self, config: &GuiConfig) -> Result<()> {
        // ... 启动逻辑
        
        if let Some(rpc) = &self.rpc_server {
            rpc.send_status(ProxyStatus::Running, 0)?;
        }
        
        Ok(())
    }
}
```

---

## Qt GUI 实现

### 核心类说明

#### ProcessManager

**职责**: 管理 Rust 后端进程

```cpp
class ProcessManager {
    bool start(const QJsonObject &config);  // 启动后端并发送配置
    void stop();                             // 停止后端
    void sendCommand(QString method, ...);   // 发送 JSON-RPC 命令
    
signals:
    void statusChanged(ProxyStatus status);
    void logReceived(QString level, QString msg, QString ts);
    void statisticsUpdated(Statistics stats);
};
```

**关键实现**:
- 使用 `QProcess` 启动 `ech-workers-rs.exe --json-rpc`
- 通过 `stdin` 写入 JSON 命令
- 从 `stdout` 逐行读取 JSON 响应
- 解析 `event` 类型并发出 Qt 信号

#### ConfigManager

**职责**: 配置文件加载/保存

```cpp
QJsonObject loadConfig();           // 从 %APPDATA% 加载
bool saveConfig(QJsonObject cfg);   // 保存到 JSON 文件
```

**配置路径**:
- Windows: `C:\Users\<用户>\AppData\Roaming\ech-workers-rs\config.json`

#### MainWindow

**职责**: 主窗口界面

**功能**:
- 3 个 Tab: 状态、设置、日志
- 启动/停止按钮
- 实时刷新统计数据
- 响应系统托盘事件

#### TrayManager

**职责**: 系统托盘管理

```cpp
void show();
void updateStatus(bool running);  // 更新图标和提示文字

signals:
    void activated();                 // 点击托盘图标
    void actionTriggered(QString);    // 菜单项触发
```

---

## 构建流程

### Windows (Visual Studio)

```powershell
# 1. 设置 Qt 路径
set Qt6_DIR=C:\Qt\6.7.0\msvc2019_64\lib\cmake\Qt6

# 2. 构建 Rust 后端
cd ech-workers-rs
cargo build --release

# 3. 构建 Qt GUI
cd ..\qt-gui
mkdir build && cd build
cmake .. -G "Visual Studio 17 2022" -DCMAKE_PREFIX_PATH=%Qt6_DIR%
cmake --build . --config Release

# 4. 部署 Qt DLL
windeployqt Release\ech-workers-gui.exe

# 5. 复制后端
copy ..\..\ech-workers-rs\target\release\ech-workers-rs.exe Release\
```

### 使用构建脚本

```powershell
cd qt-gui
build.bat
```

---

## 与原 egui GUI 功能对比

| 功能 | egui (Rust) | Qt (C++) | 状态 |
|------|-------------|----------|------|
| 启动/停止代理 | ✅ | ✅ | 完成 |
| 状态监控 | ✅ | ✅ | 完成 |
| 流量统计 | ✅ | ✅ | 完成 |
| 配置管理 | ✅ | ✅ | 完成 |
| 实时日志 | ✅ | ✅ | 完成 |
| 系统托盘 | ✅ | ✅ | 完成 |
| TUN 模式 | ⚠️ 部分 | ⚠️ 部分 | 待完善 |
| 开机自启 | ❌ | ❌ | 待实现 |
| 日志过滤 | ✅ | ❌ | 待实现 |

---

## 关键技术点

### 1. 异步通信

- **Rust**: Tokio 异步 runtime
- **Qt**: 使用 `QProcess::readyReadStandardOutput` 信号处理异步输出

### 2. 进程生命周期

- GUI 关闭时自动停止后端进程
- 异常崩溃时通过 `QProcess::errorOccurred` 捕获
- 心跳机制: 每 5 秒发送 `get_status` 命令

### 3. JSON 解析

```cpp
QJsonDocument doc = QJsonDocument::fromJson(line);
QJsonObject obj = doc.object();

if (obj.contains("event")) {
    QString event = obj["event"].toString();
    QJsonObject data = obj["data"].toObject();
    handleEvent(event, data);
}
```

### 4. 配置持久化

**格式**: JSON (与 egui 的 TOML 不同)
**结构**:
```json
{
  "basic": {...},
  "ech": {...},
  "advanced": {...},
  "app": {...}
}
```

---

## 下一步工作

### 必需
1. ✅ 完成 Rust RPC 模块集成
2. ⬜ 测试 GUI ↔ Backend 通信
3. ⬜ 添加错误处理和重连逻辑

### 优化
4. ⬜ 添加托盘图标资源文件
5. ⬜ 实现开机自启 (注册表 / 快捷方式)
6. ⬜ 日志级别过滤功能
7. ⬜ 流量图表 (QCustomPlot)

### 文档
8. ⬜ 用户手册
9. ⬜ 安装程序 (NSIS / Inno Setup)

---

## 常见问题

### Q1: 为什么使用 JSON 而不是 TOML?
**A**: Qt C++ 没有内置 TOML 解析器，JSON 通过 `QJsonDocument` 原生支持。

### Q2: 如何调试 JSON-RPC 通信?
**A**: 
- Rust 端: 在 `handle_request` 中添加 `debug!()` 日志
- Qt 端: 在 `onReadyReadStandardOutput` 中打印 `qDebug() << line`

### Q3: 为何不使用 HTTP API 通信?
**A**: stdin/stdout 更轻量，无需额外端口，进程生命周期管理更简单。

---

## 参考资料

- **Qt Documentation**: https://doc.qt.io/qt-6/
- **QProcess**: https://doc.qt.io/qt-6/qprocess.html
- **JSON-RPC 2.0 Specification**: https://www.jsonrpc.org/specification
- **Rust serde_json**: https://docs.rs/serde_json/
