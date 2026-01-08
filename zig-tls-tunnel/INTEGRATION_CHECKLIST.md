# Rust Integration Checklist

## ✅ 模块已就绪

### 构建产物
- ✅ `zig-out/lib/libzig-tls-tunnel.a` (~9KB) - Zig 代码
- ✅ `vendor/boringssl/build/libssl.a` (~31MB) - BoringSSL
- ✅ `vendor/boringssl/build/libcrypto.a` (~32MB) - BoringSSL

**总大小**: ~63MB (分开链接)

### 核心功能
- ✅ TLS 1.3 握手
- ✅ ECH 支持（4个 API）
- ✅ Firefox 120 指纹
- ✅ 降级攻击防护
- ✅ C API 导出

---

## 集成步骤

### 1. 复制 Zig 模块到 Rust 项目

```bash
# 在你的 Rust 项目根目录
mkdir -p zig-tls-tunnel
cp -r /path/to/zig-tls-tunnel/* zig-tls-tunnel/
```

### 2. 创建 FFI 绑定

文件: `src/ffi.rs`

```rust
// 复制 RUST_INTEGRATION.md 中的 FFI 定义
```

### 3. 创建安全包装

文件: `src/tunnel.rs`

```rust
// 复制 RUST_INTEGRATION.md 中的 Wrapper 代码
```

### 4. 配置构建

文件: `build.rs`

```rust
fn main() {
    println!("cargo:rustc-link-search=native=zig-tls-tunnel/zig-out/lib");
    println!("cargo:rustc-link-lib=static=zig-tls-tunnel");
    
    println!("cargo:rustc-link-search=native=zig-tls-tunnel/vendor/boringssl/build");
    println!("cargo:rustc-link-lib=static=ssl");
    println!("cargo:rustc-link-lib=static=crypto");
    
    println!("cargo:rustc-link-lib=dylib=stdc++");
}
```

### 5. 实现 DNS 查询

```rust
async fn query_ech_config(domain: &str) -> Result<Vec<u8>> {
    // 使用 trust-dns 或 hickory-dns
    // 查询 HTTPS RR (type 65)
    // 提取 ech= 参数
    // 解码 base64
    // 返回 Vec<u8>
}
```

### 6. 使用示例

```rust
// 获取 ECH config
let ech_config = query_ech_config("example.com").await?;

// 配置
let config = TunnelConfig {
    host: "example.com".to_string(),
    port: 443,
    ech_config: Some(ech_config),
    enforce_ech: true,
    use_firefox_profile: true,
    ..Default::default()
};

// 连接
let mut tunnel = TlsTunnel::connect(config)?;

// 使用
tunnel.write_all(b"GET / HTTP/1.1\r\n...")?;
let mut response = Vec::new();
tunnel.read_to_end(&mut response)?;
```

---

## C API 接口

### 配置结构

```c
struct TlsTunnelConfig {
    const char* host;
    uint16_t port;
    uint8_t _padding1[6];
    
    const uint8_t* ech_config;
    size_t ech_config_len;
    
    bool auto_ech;
    bool enforce_ech;
    bool use_firefox_profile;
    uint8_t _padding2[5];
    
    uint32_t connect_timeout_ms;
    uint32_t handshake_timeout_ms;
};
```

### 函数

```c
// 创建连接
TlsTunnel* tls_tunnel_create(
    const TlsTunnelConfig* config,
    TlsError* out_error
);

// 获取文件描述符
int tls_tunnel_get_fd(TlsTunnel* tunnel);

// 读写数据
TlsError tls_tunnel_read(TlsTunnel* tunnel, uint8_t* buffer, size_t len, size_t* out_read);
TlsError tls_tunnel_write(TlsTunnel* tunnel, const uint8_t* data, size_t len, size_t* out_written);

// 获取连接信息
TlsError tls_tunnel_get_info(TlsTunnel* tunnel, TlsInfo* out_info);

// 清理
void tls_tunnel_close(TlsTunnel* tunnel);
void tls_tunnel_destroy(TlsTunnel* tunnel);
```

### 错误码

```c
enum TlsError {
    Success = 0,
    InvalidConfig = -1,
    ConnectionFailed = -2,
    HandshakeFailed = -3,
    EchNotAccepted = -4,  // 重要：ECH 降级攻击
    OutOfMemory = -5,
    IoError = -6,
    SslError = -7,
};
```

---

## 关键配置

### 推荐配置（安全）

```rust
TunnelConfig {
    host: "example.com".to_string(),
    port: 443,
    ech_config: Some(ech_config),  // 从 DNS 获取
    auto_ech: false,               // Rust 负责 DNS
    enforce_ech: true,             // 强制验证
    use_firefox_profile: true,     // Firefox 指纹
    connect_timeout_ms: 10000,
    handshake_timeout_ms: 10000,
}
```

### 测试配置（无 ECH）

```rust
TunnelConfig {
    host: "example.com".to_string(),
    port: 443,
    ech_config: None,              // 无 ECH
    auto_ech: false,
    enforce_ech: false,            // 不强制
    use_firefox_profile: true,
    ..Default::default()
}
```

---

## 错误处理

### 必须处理的错误

```rust
match TlsTunnel::connect(config) {
    Err(TlsError::EchNotAccepted) => {
        // 🚨 ECH 降级攻击！
        // 不要回退，记录并报警
        log::error!("ECH downgrade attack detected!");
        metrics.increment("ech_downgrade_attacks");
        return Err("ECH required but not accepted");
    }
    Err(e) => {
        log::error!("TLS connection failed: {:?}", e);
        return Err(e);
    }
    Ok(tunnel) => tunnel,
}
```

---

## 性能优化

### 1. 连接池

```rust
use deadpool::managed::{Manager, Pool};

struct TlsTunnelManager {
    config: TunnelConfig,
}

impl Manager for TlsTunnelManager {
    type Type = TlsTunnel;
    type Error = TlsError;
    
    async fn create(&self) -> Result<TlsTunnel, TlsError> {
        TlsTunnel::connect(self.config.clone())
    }
    
    async fn recycle(&self, tunnel: &mut TlsTunnel) -> Result<(), TlsError> {
        // 检查连接是否还活着
        Ok(())
    }
}
```

### 2. 异步 I/O

```rust
use tokio::io::{AsyncRead, AsyncWrite};

// 将 TlsTunnel 包装为 tokio 类型
pub struct AsyncTlsTunnel {
    inner: TlsTunnel,
}

impl AsyncRead for AsyncTlsTunnel {
    // 实现异步读
}

impl AsyncWrite for AsyncTlsTunnel {
    // 实现异步写
}
```

---

## 监控指标

```rust
// 连接指标
metrics.increment("tls_connections_total");
metrics.increment("tls_connections_success");

// ECH 指标
if info.used_ech {
    metrics.increment("ech_accepted");
} else if config.ech_config.is_some() {
    metrics.increment("ech_rejected");  // 可能是攻击
}

// 性能指标
metrics.histogram("tls_handshake_duration_ms", duration);
```

---

## 测试

### 单元测试

```rust
#[test]
fn test_basic_connection() {
    let config = TunnelConfig {
        host: "example.com".to_string(),
        port: 443,
        use_firefox_profile: true,
        ..Default::default()
    };
    
    let tunnel = TlsTunnel::connect(config).unwrap();
    let info = tunnel.info().unwrap();
    
    assert_eq!(info.protocol_version, 0x0304);
}
```

### 集成测试

```rust
#[tokio::test]
async fn test_with_ech() {
    let ech_config = query_ech_config("cloudflare.com").await.unwrap();
    
    let config = TunnelConfig {
        host: "cloudflare.com".to_string(),
        port: 443,
        ech_config: Some(ech_config),
        enforce_ech: true,
        use_firefox_profile: true,
        ..Default::default()
    };
    
    let tunnel = TlsTunnel::connect(config).unwrap();
    let info = tunnel.info().unwrap();
    
    assert!(info.used_ech);
}
```

---

## 故障排查

### 链接错误

```bash
# 检查库文件
ls -lh zig-tls-tunnel/zig-out/lib/libzig-tls-tunnel.a
ls -lh zig-tls-tunnel/vendor/boringssl/build/libssl.a

# 检查符号
nm zig-tls-tunnel/zig-out/lib/libzig-tls-tunnel.a | grep tls_tunnel_create
```

### 运行时错误

```bash
# 启用日志
RUST_LOG=debug cargo run

# 检查 ECH 配置
echo $ECH_CONFIG | base64 -d | xxd
```

---

## 安全检查清单

- [ ] `enforce_ech = true` 当使用 ECH 时
- [ ] 不在 `EchNotAccepted` 时回退
- [ ] 验证 DNS HTTPS RR（DNSSEC）
- [ ] 监控降级攻击
- [ ] 使用 Firefox 指纹
- [ ] 定期更新 ECH 配置
- [ ] 记录所有 TLS 错误

---

## 文档

| 文档 | 说明 |
|------|------|
| `RUST_INTEGRATION.md` | 详细集成指南 |
| `FINGERPRINT.md` | Firefox 指纹说明 |
| `ECH_STRATEGY.md` | ECH 策略 |
| `ECH_DOWNGRADE_PROTECTION.md` | 降级防护 |
| `FINAL_SUMMARY.md` | 模块总结 |

---

## 下一步

1. ✅ 复制 Zig 模块到 Rust 项目
2. ✅ 创建 FFI 绑定
3. ✅ 实现 DNS HTTPS RR 查询
4. ✅ 集成到 HTTP 客户端
5. ✅ 添加监控和日志
6. ✅ 测试和部署

---

**状态**: ✅ 模块已就绪，可以开始集成
