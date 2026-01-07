# ECH 安全策略

## 核心原则

**这是一个纯粹的 ECH 客户端，不会回退到普通 TLS。**

## 安全保证

### 1. 无 GREASE ECH ❌

```zig
// zig-tls-tunnel/src/ssl.zig
// Note: We do NOT use ECH GREASE
// Reason: GREASE ECH exposes intent without protection
```

**原因**：
- GREASE ECH 会暴露使用 ECH 的意图
- 即使没有真实 ECH 配置也会发送 GREASE
- 可能被 DPI 识别和阻断

**策略**：
- ✅ 只在有真实 ECH 配置时才发送 ECH 扩展
- ✅ 遵循 Firefox 策略（不使用 GREASE）

### 2. 无自动回退 ❌

```rust
// src/transport/connection.rs
pub async fn establish_ech_tls(..., use_ech: bool) -> Result<TlsTunnel> {
    let config = if use_ech {
        // ECH 模式：必须查询到配置
        let ech_config = ech::query_ech_config(&host, doh_server).await
            .map_err(|e| {
                Error::Dns(format!("ECH query failed (no fallback): {}", e))
            })?;
        
        // enforce_ech = true: 强制验证 ECH
        TunnelConfig::new(&host, port).with_ech(ech_config, true)
    } else {
        // 非 ECH 模式：仅用于测试
        TunnelConfig::new(&host, port)
    };
    
    // ...
}
```

**原因**：
- 回退到普通 TLS 会暴露 SNI
- 可能被 DPI 识别和阻断
- 违背了使用 ECH 的初衷

**策略**：
- ✅ ECH 查询失败 → 连接失败
- ✅ ECH 未被接受 → 连接失败
- ✅ 不会静默回退到普通 TLS

### 3. 强制验证 ✅

```zig
// zig-tls-tunnel/src/tunnel.zig
if (ech_configured and config.enforce_ech) {
    const ech_accepted = ech.wasAccepted(self.ssl_conn);
    if (!ech_accepted) {
        std.log.err("ECH configured but NOT accepted - possible downgrade attack!", .{});
        return error.EchNotAccepted;
    }
}
```

```rust
// src/transport/connection.rs
if use_ech {
    let info = tunnel.info()?;
    if !info.used_ech {
        return Err(Error::Dns(
            "ECH not accepted by server (possible downgrade attack)".into()
        ));
    }
}
```

**原因**：
- 检测降级攻击（DPI 剥离 ECH 扩展）
- 确保 ECH 真正被使用
- 防止中间人攻击

**策略**：
- ✅ 握手后立即检查 `SSL_ech_accepted()`
- ✅ ECH 未被接受 → 立即失败
- ✅ 记录错误日志

## 使用模式

### 模式 1: 严格 ECH 模式（推荐）

```bash
./ech-workers-rs proxy \
  --listen 127.0.0.1:1080 \
  --server example.com:8443/ws \
  --token mytoken \
  --ech                    # 启用 ECH
  --yamux
```

**行为**：
1. 查询 ECH 配置（通过 DoH）
2. 如果查询失败 → **连接失败**
3. 建立 TLS 连接（带 ECH）
4. 如果 ECH 未被接受 → **连接失败**
5. 只有 ECH 成功才继续

### 模式 2: 非 ECH 模式（仅用于测试）

```bash
./ech-workers-rs proxy \
  --listen 127.0.0.1:1080 \
  --server example.com:8443/ws \
  --token mytoken \
  --no-ech                 # 禁用 ECH
  --yamux
```

**行为**：
1. 不查询 ECH 配置
2. 建立普通 TLS 连接
3. SNI 明文传输

**⚠️ 警告**：此模式仅用于测试，不推荐生产使用。

## 失败场景

### 场景 1: DoH 查询失败

```
Error: ECH query failed (no fallback): DNS query failed: No ECH config found
```

**原因**：
- 域名不支持 ECH
- DoH 服务器无法访问
- 网络问题

**解决**：
- 确认域名支持 ECH：`dig HTTPS example.com`
- 尝试不同的 DoH 服务器
- 检查网络连接

### 场景 2: ECH 未被接受

```
Error: ECH not accepted by server (possible downgrade attack or misconfiguration)
```

**原因**：
- 服务器不支持 ECH
- ECH 配置过期
- DPI 剥离了 ECH 扩展（降级攻击）

**解决**：
- 确认服务器支持 ECH
- 重新查询 ECH 配置
- 检查网络中是否有 DPI 设备

### 场景 3: 连接超时

```
Error: Connection timeout
```

**原因**：
- 服务器无法访问
- 防火墙阻断
- 网络问题

**解决**：
- 检查服务器地址
- 检查防火墙规则
- 尝试不同的网络

## 安全级别

### 🔒 高安全（推荐）

```rust
Config {
    use_ech: true,           // 启用 ECH
    enforce_ech: true,       // 强制验证（默认）
    use_firefox_profile: true, // Firefox 指纹（默认）
}
```

**保证**：
- ✅ SNI 加密
- ✅ 无 GREASE ECH
- ✅ 降级攻击检测
- ✅ Firefox 指纹

### ⚠️ 中安全（不推荐）

```rust
Config {
    use_ech: false,          // 禁用 ECH
}
```

**风险**：
- ❌ SNI 明文传输
- ❌ 可能被 DPI 识别
- ❌ 无隐私保护

## 对比其他实现

### Chrome/Chromium

```
❌ 使用 GREASE ECH（即使没有真实配置）
❌ 暴露使用 ECH 的意图
⚠️ 可能被 DPI 识别
```

### Firefox

```
✅ 不使用 GREASE ECH
✅ 只在有真实配置时发送
✅ 更安全的策略
```

### 本实现

```
✅ 遵循 Firefox 策略
✅ 不使用 GREASE ECH
✅ 强制验证 ECH
✅ 无自动回退
✅ 降级攻击检测
```

## 配置选项

### enforce_ech（默认：true）

```rust
TunnelConfig::new(host, port)
    .with_ech(ech_config, true)  // enforce_ech = true
```

**行为**：
- `true`: ECH 未被接受 → 连接失败（推荐）
- `false`: ECH 未被接受 → 继续连接（不推荐）

### use_ech（默认：true）

```bash
--ech      # 启用 ECH（推荐）
--no-ech   # 禁用 ECH（仅测试）
```

**行为**：
- `true`: 查询 ECH 配置，强制使用
- `false`: 不查询 ECH，使用普通 TLS

## 日志示例

### 成功的 ECH 连接

```
INFO  Establishing ECH + TLS connection to crypto.cloudflare.com:443
DEBUG Querying ECH config for crypto.cloudflare.com via https://cloudflare-dns.com/dns-query
INFO  ✅ Got ECH config: 71 bytes
INFO  ✅ ECH successfully negotiated
```

### ECH 查询失败

```
INFO  Establishing ECH + TLS connection to example.com:443
DEBUG Querying ECH config for example.com via https://cloudflare-dns.com/dns-query
ERROR ECH query failed (no fallback): DNS query failed: No ECH config found
```

### ECH 未被接受

```
INFO  Establishing ECH + TLS connection to example.com:443
DEBUG Querying ECH config for example.com via https://cloudflare-dns.com/dns-query
INFO  ✅ Got ECH config: 71 bytes
ERROR ECH not accepted by server (possible downgrade attack or misconfiguration)
```

## 测试

### 测试 ECH 支持

```bash
# 测试域名是否支持 ECH
dig HTTPS crypto.cloudflare.com

# 测试 DoH 查询
./ech-workers-rs test-doh crypto.cloudflare.com

# 测试 ECH 连接
./ech-workers-rs connect crypto.cloudflare.com
```

### 验证无 fallback

```bash
# 测试不支持 ECH 的域名（应该失败）
./ech-workers-rs connect www.google.com
# 预期：Error: ECH query failed (no fallback)

# 测试 ECH 被剥离的情况（应该失败）
# （需要模拟 DPI 环境）
```

## 总结

**这是一个纯粹的 ECH 客户端**：

1. ✅ 不使用 GREASE ECH
2. ✅ 不会自动回退到普通 TLS
3. ✅ 强制验证 ECH 状态
4. ✅ 检测降级攻击
5. ✅ 遵循 Firefox 安全策略

**如果 ECH 失败，连接就失败。没有妥协。**
