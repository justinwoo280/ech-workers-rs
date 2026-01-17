# WinTun Integration Guide

## 概述

**WinTun** 是 Windows 平台的轻量级 TUN 虚拟网卡驱动，用于实现全局透明代理（TUN模式）。

---

## 依赖信息

| 组件 | 版本 | 来源 | 许可证 |
|------|------|------|--------|
| WinTun | 0.14.1 | [wintun.net](https://www.wintun.net/) | GPL-2.0 |
| 文件 | wintun.dll | AMD64 (x86_64) | - |
| 大小 | ~150 KB | - | - |

---

## 自动集成（CI/CD）

### GitHub Actions Workflow

```yaml
- name: Download WinTun (Windows only)
  if: matrix.os == 'windows-latest'
  run: |
    $wintunVersion = "0.14.1"
    $wintunUrl = "https://www.wintun.net/builds/wintun-$wintunVersion.zip"
    Invoke-WebRequest -Uri $wintunUrl -OutFile wintun.zip
    Expand-Archive -Path wintun.zip -DestinationPath wintun-temp
    Copy-Item wintun-temp\wintun\bin\amd64\wintun.dll ech-workers-rs\target\release\
    Remove-Item wintun.zip, wintun-temp -Recurse -Force
```

### 发布包结构

```
ech-workers-gui-windows-x64.zip
├── ech-workers-gui.exe      # Qt GUI
├── ech-workers-rs.exe       # Rust Backend
├── wintun.dll               # WinTun Driver
├── Qt6Core.dll
├── Qt6Widgets.dll
└── ...
```

---

## 手动安装

### 1. 下载 WinTun

```powershell
# PowerShell
Invoke-WebRequest -Uri https://www.wintun.net/builds/wintun-0.14.1.zip -OutFile wintun.zip
Expand-Archive wintun.zip -DestinationPath wintun
```

### 2. 复制到应用目录

```cmd
copy wintun\wintun\bin\amd64\wintun.dll "C:\Program Files\ECH Workers RS\"
```

**重要**: `wintun.dll` 必须与 `ech-workers-rs.exe` 在同一目录。

---

## TUN 模式使用

### 启用条件

1. ✅ `wintun.dll` 存在于可执行文件目录
2. ✅ **管理员权限**运行（创建虚拟网卡需要）
3. ✅ 配置中启用 TUN 模式

### GUI 配置

**Settings → Basic → Enable TUN Mode**

```json
{
  "basic": {
    "enable_tun": true,
    "listen_addr": "127.0.0.1:1080"  // TUN模式下SOCKS5仍可用
  }
}
```

### TUN 网络参数

| 参数 | 默认值 | 说明 |
|------|--------|------|
| IP 地址 | `10.0.85.2` | TUN虚拟网卡地址 |
| 网关 | `10.0.85.1` | 默认网关 |
| 子网掩码 | `255.255.255.0` | /24 |
| MTU | `1500` | 最大传输单元 |

---

## 运行时检查

### Rust Backend 日志

```
[INFO] TUN device created: wintun
[INFO] TUN mode enabled: 10.0.85.2/24
[INFO] Default route configured via 10.0.85.1
```

### Windows 网卡验证

```powershell
# 查看虚拟网卡
ipconfig /all | findstr -i wintun

# 查看路由表
route print | findstr 10.0.85
```

### 测试连接

```cmd
# TUN模式下所有流量自动走代理
curl https://ipinfo.io/json
```

---

## 故障排查

### 问题1: 找不到 wintun.dll

**错误**:
```
Error: Failed to create TUN device: The system cannot find the file specified.
```

**解决**:
```powershell
# 验证文件存在
Test-Path ".\wintun.dll"

# 复制到正确位置
Copy-Item wintun.dll (Split-Path (Get-Process ech-workers-rs).Path)
```

---

### 问题2: 权限不足

**错误**:
```
Error: Access is denied (os error 5)
```

**解决**:
```
右键 ech-workers-gui.exe → 以管理员身份运行
```

或通过快捷方式属性设置：
```
属性 → 兼容性 → 以管理员身份运行此程序 ✅
```

---

### 问题3: 网络断开

**症状**: TUN模式下无法访问网络

**排查**:
```powershell
# 检查TUN网卡状态
Get-NetAdapter | Where-Object {$_.InterfaceDescription -like "*wintun*"}

# 检查路由
route print -4

# 重置网络栈
netsh int ip reset
netsh winsock reset
```

---

## 性能特性

### WinTun vs. TAP-Windows

| 特性 | WinTun | TAP-Windows |
|------|--------|-------------|
| **内核态** | ✅ | ✅ |
| **吞吐量** | **~5 Gbps** | ~1 Gbps |
| **延迟** | **<1ms** | ~5ms |
| **CPU占用** | **低** | 中等 |
| **维护状态** | 活跃 | 停滞 |

---

## 安全性

### DLL 劫持防护

**威胁**: 恶意 `wintun.dll` 替换

**防护**:
1. 仅从官方源下载 ([wintun.net](https://www.wintun.net/))
2. 验证文件签名：
   ```powershell
   Get-AuthenticodeSignature wintun.dll | Format-List
   # Subject: CN=WireGuard LLC
   ```
3. 校验SHA256：
   ```powershell
   Get-FileHash wintun.dll -Algorithm SHA256
   # 与官方发布的hash对比
   ```

---

## 合规性

### GPL-2.0 许可证遵守

**WinTun 许可证要求**:
- ✅ 分发时保留版权声明
- ✅ 提供源代码或获取途径（官方链接）
- ✅ 说明修改内容（我们未修改原始DLL）

**许可证文件**: 发布包中包含 `LICENSE-WINTUN.txt`

---

## 高级配置

### 自定义 TUN 参数

修改 `src/tun/device.rs`:

```rust
pub struct TunConfig {
    pub name: String,
    pub address: IpAddr,        // 默认 10.0.85.2
    pub netmask: IpAddr,        // 默认 255.255.255.0
    pub gateway: IpAddr,        // 默认 10.0.85.1
    pub mtu: u16,               // 默认 1500
}
```

### DNS 劫持（FakeDNS）

TUN模式下自动启用 FakeDNS:

```rust
// src/fakedns/mod.rs
const FAKE_DNS_RANGE: &str = "198.18.0.0/16";  // RFC 3330
```

---

## 卸载

### 清理虚拟网卡

```powershell
# 停止应用后虚拟网卡自动移除
# 如果残留，手动删除：
Get-NetAdapter | Where-Object {$_.InterfaceDescription -like "*wintun*"} | Remove-NetAdapter
```

### 恢复路由表

```cmd
# TUN模式会修改默认路由，停止后自动恢复
# 手动恢复：
route delete 0.0.0.0
route add 0.0.0.0 mask 0.0.0.0 <原网关IP>
```

---

## 参考资源

- **WinTun 官网**: https://www.wintun.net/
- **源代码**: https://git.zx2c4.com/wintun/
- **文档**: https://git.zx2c4.com/wintun/about/
- **WireGuard**: https://www.wireguard.com/ (WinTun开发者)

---

## 更新日志

| 日期 | 版本 | 变更 |
|------|------|------|
| 2026-01-17 | 0.14.1 | 集成到CI/CD工作流 |
| - | - | 自动下载并打包到发布包 |
| - | - | 添加完整的故障排查文档 |
