# ECH Workers RS - Qt GUI

基于 Qt 6 的 C++ 图形界面，通过 stdin/stdout JSON-RPC 与 Rust 后端通信。

## 架构设计

```
Qt GUI (C++)  ←→ stdin/stdout (JSON-RPC) ←→ Rust Backend
```

### 通信协议

**GUI → Rust (stdin)**
```json
{"id":1,"method":"start","params":{"basic":{"listen_addr":"127.0.0.1:1080",...}}}
{"id":2,"method":"stop"}
{"id":3,"method":"get_status"}
```

**Rust → GUI (stdout)**
```json
{"id":1,"result":{"status":"ok"}}
{"event":"status","data":{"status":"running","uptime_secs":120}}
{"event":"log","data":{"level":"info","message":"...","timestamp":"..."}}
{"event":"stats","data":{"upload_bytes":1024,"download_bytes":2048,...}}
```

## 功能特性

- **状态面板**: 实时显示连接状态、运行时间、流量统计
- **设置面板**: 完整配置界面（基本/ECH/高级/应用设置）
- **日志面板**: 实时日志查看、颜色编码、自动滚动
- **系统托盘**: Windows 托盘图标、右键菜单、最小化到托盘
- **配置持久化**: JSON 格式配置文件自动保存

## 构建要求

- **CMake** 3.16+
- **Qt 6.2+** (Core, Gui, Widgets)
- **C++17 编译器** (MSVC 2019+, GCC 9+, Clang 10+)
- **Rust 后端** (`ech-workers-rs.exe`)

## Windows 构建步骤

### 1. 安装 Qt 6

```powershell
# 使用 Qt Online Installer
# 下载: https://www.qt.io/download-qt-installer

# 或使用 vcpkg
vcpkg install qt6-base:x64-windows
```

### 2. 构建 Rust 后端

```powershell
cd ..\ech-workers-rs
cargo build --release
copy target\release\ech-workers-rs.exe ..\qt-gui\build\
```

### 3. 构建 Qt GUI

```powershell
cd qt-gui
mkdir build
cd build

# 设置 Qt 路径 (根据实际安装路径)
set Qt6_DIR=C:\Qt\6.7.0\msvc2019_64\lib\cmake\Qt6

cmake .. -G "Visual Studio 17 2022" -DCMAKE_PREFIX_PATH=%Qt6_DIR%
cmake --build . --config Release

# 复制 Qt DLL
windeployqt Release\ech-workers-gui.exe
```

### 4. 运行

```powershell
.\Release\ech-workers-gui.exe
```

## 项目结构

```
qt-gui/
├── CMakeLists.txt          # CMake 构建配置
├── include/
│   ├── mainwindow.h        # 主窗口
│   ├── processmanager.h    # 后端进程管理
│   ├── configmanager.h     # 配置管理
│   ├── traymanager.h       # 系统托盘
│   └── settingsdialog.h    # 设置对话框
├── src/
│   ├── main.cpp            # 程序入口
│   ├── mainwindow.cpp
│   ├── processmanager.cpp
│   ├── configmanager.cpp
│   ├── traymanager.cpp
│   └── settingsdialog.cpp
└── resources/
    └── resources.qrc       # Qt 资源文件
```

## 开发说明

### ProcessManager

负责管理 Rust 后端进程的生命周期：

- **start()**: 启动后端进程，发送 `start` 命令
- **stop()**: 停止后端进程
- **sendCommand()**: 通过 stdin 发送 JSON-RPC 命令
- **onReadyReadStandardOutput()**: 解析 stdout 返回的 JSON 响应

### ConfigManager

管理配置文件的加载/保存：

- **loadConfig()**: 从 `%APPDATA%\ech-workers-rs\config.json` 加载
- **saveConfig()**: 保存配置到文件
- **createDefaultConfig()**: 生成默认配置

### 与 egui GUI 对比

| 特性 | egui (Rust) | Qt (C++) |
|------|-------------|----------|
| 语言 | Rust | C++ |
| 框架 | egui/eframe | Qt 6 |
| 后端通信 | 直接调用 | stdin/stdout IPC |
| 系统托盘 | tray-icon | QSystemTrayIcon |
| 配置格式 | TOML | JSON |
| 构建工具 | Cargo | CMake |
| 二进制大小 | ~15MB | ~8MB (需要 Qt DLL) |

## 待完成

- [ ] 添加托盘图标资源
- [ ] 实现开机自启功能
- [ ] TUN 模式完整支持
- [ ] 更新检查
- [ ] 日志级别过滤
- [ ] 流量图表显示
