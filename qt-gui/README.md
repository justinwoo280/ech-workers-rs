# ECH Workers RS - Qt GUI

基于 Qt 6 的 C++ 图形界面，通过 stdin/stdout JSON-RPC 与 Rust 后端通信。

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
