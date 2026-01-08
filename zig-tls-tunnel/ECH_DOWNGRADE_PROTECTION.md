# ECH Downgrade Attack Protection

## 威胁模型

### DPI/防火墙降级攻击

```
Client                    DPI/Firewall              Server
  |                            |                        |
  |--ClientHello+ECH---------->|                        |
  |  (包含 ECH extension)       |                        |
  |                            |                        |
  |                       [攻击者篡改]                  |
  |                       删除 ECH extension            |
  |                            |                        |
  |                            |--ClientHello---------->|
  |                            |  (无 ECH)              |
  |                            |                        |
  |<---------------------------|<---ServerHello---------|
  |                            |                        |
  |  ❌ 连接成功               |                        |
  |  ❌ 但 SNI 是明文的！      |                        |
  |  ❌ 用户以为使用了 ECH     |                        |
```

### 攻击后果

1. **隐私泄露**: SNI 以明文传输，DPI 可以看到访问的域名
2. **审查绕过失败**: 防火墙可以基于 SNI 进行封锁
3. **用户误判**: 用户以为使用了 ECH，实际上没有

---

## 防护机制

### 1. ECH 强制验证

```zig
pub const TunnelConfig = struct {
    // ...
    enforce_ech: bool = true,  // 默认启用
};
```

**逻辑**:
```zig
// 1. 配置 ECH
if (config.ech_config) |ech_cfg| {
    try ech.configure(self.ssl_conn, ech_cfg);
    ech_configured = true;
}

// 2. 执行握手
try performHandshake(self.ssl_conn, config.handshake_timeout_ms);

// 3. 验证 ECH 是否被接受
if (ech_configured and config.enforce_ech) {
    const ech_accepted = ech.wasAccepted(self.ssl_conn);
    if (!ech_accepted) {
        // ECH 配置了但没被接受 - 可能是降级攻击！
        return error.EchNotAccepted;
    }
}
```

### 2. 失败场景

当 `enforce_ech = true` 且 ECH 未被接受时，连接会失败：

```
error: EchNotAccepted
```

**可能原因**:
1. ✅ **DPI 降级攻击** - 防火墙删除了 ECH extension
2. ⚠️ **服务器不支持 ECH** - 配置错误
3. ⚠️ **ECH config 过期** - 需要更新 DNS 记录

---

## 使用场景

### 场景 1: 强制 ECH (推荐)

```zig
const config = TunnelConfig{
    .host = "example.com",
    .port = 443,
    .ech_config = ech_config_bytes,
    .enforce_ech = true,  // 默认值
    .profile = .Chrome133,
};

const tunnel = try Tunnel.create(allocator, config);
// 如果 ECH 未被接受，这里会返回 error.EchNotAccepted
```

**行为**:
- ✅ ECH 被接受 → 连接成功
- ❌ ECH 未被接受 → 连接失败（防止降级攻击）

### 场景 2: 可选 ECH (不推荐)

```zig
const config = TunnelConfig{
    .host = "example.com",
    .port = 443,
    .ech_config = ech_config_bytes,
    .enforce_ech = false,  // 禁用强制验证
    .profile = .Chrome133,
};

const tunnel = try Tunnel.create(allocator, config);
// 即使 ECH 未被接受，连接也会成功
```

**行为**:
- ✅ ECH 被接受 → 连接成功，SNI 加密
- ⚠️ ECH 未被接受 → 连接成功，但 SNI 明文（降级）

**警告**: 这种模式不安全，只用于测试！

### 场景 3: 无 ECH

```zig
const config = TunnelConfig{
    .host = "example.com",
    .port = 443,
    .ech_config = null,  // 不使用 ECH
    .profile = .Chrome133,
};

const tunnel = try Tunnel.create(allocator, config);
// 正常连接，使用 ECH GREASE (Chrome)
```

**行为**:
- ✅ 连接成功
- ℹ️ 使用 ECH GREASE (Chrome) 或不使用 ECH (Firefox)
- ⚠️ SNI 是明文的

---

## 错误处理

### Rust 侧处理

```rust
match zig_tls_tunnel_create(&config) {
    Ok(tunnel) => {
        // ECH 成功或未配置 ECH
        println!("Connected successfully");
    }
    Err(TlsError::EchNotAccepted) => {
        // ECH 配置了但未被接受
        eprintln!("ECH downgrade attack detected!");
        eprintln!("Possible causes:");
        eprintln!("1. DPI/Firewall stripped ECH");
        eprintln!("2. Server doesn't support ECH");
        eprintln!("3. ECH config expired");
        
        // 选项 1: 重试（可能获取新的 ECH config）
        // 选项 2: 回退到无 ECH 连接（不推荐）
        // 选项 3: 报告错误给用户
    }
    Err(e) => {
        eprintln!("Connection failed: {:?}", e);
    }
}
```

### 日志输出

```
# 成功场景
info: Found ECH config via DNS HTTPS RR for example.com
info: ECH accepted by server

# 失败场景
info: Found ECH config via DNS HTTPS RR for example.com
error: ECH configured but NOT accepted - possible downgrade attack!
error: EchNotAccepted
```

---

## 安全建议

### ✅ 推荐做法

1. **默认启用 `enforce_ech`**
   ```zig
   .enforce_ech = true,  // 默认值
   ```

2. **失败时不要回退**
   ```rust
   if error == EchNotAccepted {
       return Err("ECH required but not available");
   }
   ```

3. **记录攻击尝试**
   ```rust
   if error == EchNotAccepted {
       log::warn!("Possible ECH downgrade attack detected");
       metrics.increment("ech_downgrade_attempts");
   }
   ```

### ❌ 不推荐做法

1. **禁用 `enforce_ech`**
   ```zig
   .enforce_ech = false,  // 危险！
   ```

2. **静默回退**
   ```rust
   if error == EchNotAccepted {
       // 不要这样做！
       return connect_without_ech();
   }
   ```

3. **忽略错误**
   ```rust
   let _ = tunnel.create();  // 不要忽略错误！
   ```

---

## 测试

### 模拟降级攻击

```bash
# 使用 mitmproxy 删除 ECH extension
mitmproxy --mode transparent \
  --modify-body '/ClientHello.*ECH//' \
  --listen-port 8080

# 测试
./test-ech example.com 443 <ech_config>
# 应该失败: error.EchNotAccepted
```

### 正常 ECH 测试

```bash
# 使用支持 ECH 的服务器
./test-ech cloudflare.com 443 <ech_config>
# 应该成功: ECH accepted by server
```

---

## 与其他防护的关系

### TLS 1.3 降级防护

TLS 1.3 本身有降级防护（downgrade protection）：
- ServerHello.random 的最后 8 字节包含特殊值
- 如果客户端支持 TLS 1.3 但服务器降级到 1.2，客户端会检测到

### ECH 降级防护（本文档）

ECH 降级防护是额外的：
- 检查 ECH extension 是否被接受
- 防止 DPI 删除 ECH 但保留 TLS 1.3 连接

### 两者结合

```
TLS 1.3 降级防护: 防止协议版本降级
ECH 降级防护:     防止 ECH 被删除
```

---

## 参考

- [ECH Draft - Section 11.1 (Security Considerations)](https://datatracker.ietf.org/doc/draft-ietf-tls-esni/)
- [TLS 1.3 Downgrade Protection](https://datatracker.ietf.org/doc/html/rfc8446#section-4.1.3)
- [BoringSSL ECH Implementation](https://github.com/google/boringssl/blob/master/ssl/encrypted_client_hello.cc)

---

## 总结

| 配置 | ECH Config | enforce_ech | 行为 |
|------|-----------|-------------|------|
| 安全 | ✅ | ✅ | ECH 失败 → 连接失败 |
| 不安全 | ✅ | ❌ | ECH 失败 → 连接成功（降级） |
| 正常 | ❌ | - | 不使用 ECH |

**推荐**: 始终使用 `enforce_ech = true` (默认值)
