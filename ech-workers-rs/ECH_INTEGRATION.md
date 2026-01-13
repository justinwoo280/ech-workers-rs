# ECH Integration Guide

## 概述

本项目实现了完整的 ECH (Encrypted Client Hello) 支持，通过集成 Zig TLS Tunnel 和 BoringSSL。

## 架构

```
Rust Application
    ↓
DoH Module (src/ech/doh.rs)
    ↓ ECH Config
Rust FFI Wrapper (src/tls/tunnel.rs)
    ↓ C ABI
Zig TLS Tunnel (zig-tls-tunnel/src/api.zig)
    ↓
BoringSSL (ECH + TLS 1.3)
```

## 核心组件

### 1. DoH 模块
**位置**: `src/ech/doh.rs`

查询 HTTPS RR (type 65) 并提取 ECH 配置：
```rust
let ech_config = ech::query_ech_config(
    "crypto.cloudflare.com",
    "https://cloudflare-dns.com/dns-query"
).await?;
```

### 2. FFI 绑定
**位置**: `src/tls/ffi.rs`

定义 C ABI 兼容的结构体和函数：
```rust
#[repr(C)]
pub struct TlsTunnelConfig {
    pub host: *const c_char,
    pub port: c_ushort,
    pub ech_config: *const u8,
    pub ech_config_len: usize,
    pub enforce_ech: bool,
    // ...
}

extern "C" {
    pub fn tls_tunnel_create(...) -> *mut TlsTunnel;
    pub fn tls_tunnel_get_info(...) -> TlsError;
}
```

### 3. 安全包装器
**位置**: `src/tls/tunnel.rs`

提供安全的 Rust API：
```rust
pub struct TlsTunnel { /* ... */ }

impl TlsTunnel {
    pub fn connect(config: TunnelConfig) -> Result<Self>;
    pub fn info(&self) -> Result<ConnectionInfo>;
}

impl Drop for TlsTunnel {
    fn drop(&mut self) {
        // 自动清理资源
    }
}
```

### 4. Zig TLS Tunnel
**位置**: `zig-tls-tunnel/src/api.zig`

导出 C API 供 Rust 调用：
```zig
export fn tls_tunnel_create(
    config: *const TlsTunnelConfig,
    out_error: *TlsError,
) ?*TlsTunnel {
    // 创建 TLS 连接
}
```

## 使用示例

### 基本用法

```rust
use ech_workers_rs::{ech, tls};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. 查询 ECH 配置
    let ech_config = ech::query_ech_config(
        "crypto.cloudflare.com",
        "https://cloudflare-dns.com/dns-query"
    ).await?;

    // 2. 创建 TLS 配置
    let config = tls::TunnelConfig::new("crypto.cloudflare.com", 443)
        .with_ech(ech_config, true);  // enforce_ech = true

    // 3. 建立连接
    let tunnel = tls::TlsTunnel::connect(config)?;

    // 4. 验证 ECH
    let info = tunnel.info()?;
    assert!(info.used_ech);

    Ok(())
}
```

### 运行测试

```bash
# 端到端测试
cargo run --example test_ech_e2e --release crypto.cloudflare.com

# 使用不同的 DoH 服务器
cargo run --example test_ech_e2e --release defo.ie https://dns.google/dns-query
```

## 构建

### 前置条件

- Rust 1.70+
- Zig 0.11+
- CMake (用于 BoringSSL)

### 构建步骤

```bash
# 1. 构建 BoringSSL (如果还没有)
cd zig-tls-tunnel/vendor/boringssl
mkdir -p build && cd build
cmake -GNinja -DCMAKE_BUILD_TYPE=Release ..
ninja

# 2. 构建 Zig TLS Tunnel
cd ../../..
zig build -Doptimize=ReleaseFast

# 3. 构建 Rust 项目
cd ../..
cargo build --release
```

## 配置

### TunnelConfig 选项

```rust
pub struct TunnelConfig {
    pub host: String,              // 目标主机
    pub port: u16,                 // 目标端口
    pub ech_config: Option<Vec<u8>>, // ECH 配置
    pub auto_ech: bool,            // 自动查询 ECH (Rust 中设为 false)
    pub enforce_ech: bool,         // 强制验证 ECH
    pub use_firefox_profile: bool, // 使用 Firefox 120 指纹
    pub connect_timeout_ms: u32,   // 连接超时
    pub handshake_timeout_ms: u32, // 握手超时
}
```

### DoH 服务器

支持的 DoH 提供商：
- Cloudflare: `https://cloudflare-dns.com/dns-query`
- Google: `https://dns.google/dns-query`
- Quad9: `https://dns.quad9.net/dns-query`

## 安全特性

### ECH 降级保护

当 `enforce_ech = true` 时：
- 如果 ECH 配置了但未被服务器接受，连接失败
- 防止中间人攻击剥离 ECH

### Chrome 指纹

使用 Chrome 120+ 的 TLS 指纹：
- ML-KEM (X25519MLKEM768) 抗量子密钥交换
- 完整 cipher suite 列表（TLS 1.3 + TLS 1.2）
- ALPN, OCSP, SCT, ALPS 扩展
- GREASE 和扩展随机排列

## 故障排除

### 编译错误

**问题**: 找不到 `tls_tunnel_create` 符号
```
undefined reference to `tls_tunnel_create`
```

**解决**: 确保 Zig 库已编译
```bash
cd zig-tls-tunnel
zig build -Doptimize=ReleaseFast
ls zig-out/lib/libzig-tls-tunnel.a  # 应该存在且 > 400KB
```

**问题**: OpenSSL 符号冲突
```
multiple definition of `SSL_read`
```

**解决**: 确保 reqwest 使用 rustls
```toml
reqwest = { version = "0.11", default-features = false, features = ["rustls-tls"] }
```

### 运行时错误

**问题**: ECH 未被接受
```
Error: ECH not accepted
```

**可能原因**:
1. 域名不支持 ECH
2. ECH 配置过期
3. 网络中间人剥离 ECH

**解决**:
- 验证域名支持 ECH: `dig HTTPS example.com`
- 重新查询 ECH 配置
- 检查网络环境

## 性能

### 基准测试

```
DoH 查询: ~100ms
TLS 握手: ~30ms
总延迟: ~130ms
内存使用: ~15MB
```

### 优化建议

1. **缓存 ECH 配置**: ECH 配置通常有效期较长
2. **连接池**: 复用 TLS 连接
3. **并行查询**: 同时查询多个 DoH 服务器

## 参考

- [RFC 9460: HTTPS RR](https://datatracker.ietf.org/doc/html/rfc9460)
- [draft-ietf-tls-esni-18: ECH](https://datatracker.ietf.org/doc/html/draft-ietf-tls-esni-18)
- [BoringSSL ECH](https://boringssl.googlesource.com/boringssl/)
- [Zig TLS Tunnel](./zig-tls-tunnel/README.md)
