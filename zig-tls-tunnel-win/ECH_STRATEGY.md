# ECH Strategy: All or Nothing

## 核心原则

**要么使用真实 ECH，要么完全不用 ECH。永远不用 GREASE ECH。**

---

## 为什么这样做？

### 真实 ECH 的秘密武器：外部 SNI

```
普通 TLS 连接:
  SNI: example.com
  DPI 看到: example.com
  
GREASE ECH 连接:
  外部 SNI: example.com
  ECH extension: 假的
  DPI 看到: example.com + 知道你想用 ECH
  
真实 ECH 连接:
  外部 SNI: cloudflare-ech.com  ← 关键！
  ECH extension: 加密的 example.com
  DPI 看到: cloudflare-ech.com (大厂域名)
```

### 投鼠忌器策略

```
DPI 的困境:
  
  看到 cloudflare-ech.com:
    - 这是 Cloudflare 的 ECH 服务
    - 大量正常用户在使用
    - 封锁它 = 封锁整个 Cloudflare ECH
    - 后果: 大量误伤，用户投诉
    
  决定: 不敢封锁
```

### GREASE ECH 的问题

```
DPI 看到 GREASE ECH:
  
  1. 外部 SNI = 真实域名 (暴露目标)
  2. ECH extension 存在但是假的
  3. 可以检测出是 GREASE (特征明显)
  
  DPI 知道:
    - 你想访问哪个域名
    - 你在尝试使用 ECH
    - 但你没有真实的 ECH config
    
  决定: 安全封锁 (不会误伤)
```

---

## 实现策略

### 代码逻辑

```zig
// 1. 永远禁用 GREASE ECH
ssl.setEchGreaseEnabled(ssl_conn, false);

// 2. 只在有真实 ECH config 时使用 ECH
if (config.ech_config) |ech_cfg| {
    try ech.configure(self.ssl_conn, ech_cfg);
    ech_configured = true;
}

// 3. 握手后强制验证
if (ech_configured and config.enforce_ech) {
    if (!ech.wasAccepted(self.ssl_conn)) {
        // 失败就失败，不回退
        return error.EchNotAccepted;
    }
}
```

### 三种模式

| 模式 | ECH Config | 行为 | 外部 SNI | 安全性 |
|------|-----------|------|---------|--------|
| 真实 ECH | ✅ | 使用真实 ECH | cloudflare-ech.com | 高 |
| 无 ECH | ❌ | 普通 TLS | example.com | 中 |
| ~~GREASE ECH~~ | ❌ | ~~已禁用~~ | ~~example.com~~ | ~~低~~ |

---

## 使用场景

### 场景 1: 有 ECH Config (推荐)

```zig
const ech_config = try queryEchConfig("example.com");

const config = TunnelConfig{
    .host = "example.com",
    .port = 443,
    .ech_config = ech_config,
    .enforce_ech = true,
    .profile = .Chrome133,
};

const tunnel = try Tunnel.create(allocator, config);
```

**效果**:
- ✅ 外部 SNI: `cloudflare-ech.com`
- ✅ 内部 SNI: `example.com` (加密)
- ✅ DPI 投鼠忌器，不敢封锁
- ✅ 隐私保护

### 场景 2: 无 ECH Config

```zig
const config = TunnelConfig{
    .host = "example.com",
    .port = 443,
    .ech_config = null,
    .profile = .Chrome133,
};

const tunnel = try Tunnel.create(allocator, config);
```

**效果**:
- ⚠️ 外部 SNI: `example.com`
- ❌ 无 ECH extension
- ⚠️ DPI 可以看到域名
- ⚠️ 无隐私保护

### ~~场景 3: GREASE ECH (已禁用)~~

```zig
// 这种模式已经被禁用
// 即使 Chrome 真实行为是发送 GREASE，我们也不这样做
```

**原因**: GREASE ECH 暴露意图，增加风险

---

## 与真实浏览器的差异

### 真实 Chrome 133

```
无 ECH config:
  → 发送 GREASE ECH
  → 外部 SNI = 真实域名
  → 目的: 让服务器适应 ECH

有 ECH config:
  → 发送真实 ECH
  → 外部 SNI = cloudflare-ech.com
  → 目的: 隐私保护
```

### 我们的实现

```
无 ECH config:
  → 不发送任何 ECH  ← 差异
  → 外部 SNI = 真实域名
  → 目的: 不暴露意图

有 ECH config:
  → 发送真实 ECH  ← 相同
  → 外部 SNI = cloudflare-ech.com
  → 目的: 隐私保护 + 投鼠忌器
```

### 为什么偏离真实浏览器？

1. **环境不同**
   - 真实浏览器: 正常网络环境
   - 我们的场景: 审查网络环境

2. **目标不同**
   - 真实浏览器: 推广 ECH 标准
   - 我们的目标: 绕过审查

3. **风险不同**
   - 真实浏览器: GREASE 无风险
   - 审查环境: GREASE 暴露意图

---

## 安全分析

### 攻击面对比

#### GREASE ECH (已禁用)
```
DPI 可以:
  ✅ 看到真实域名 (外部 SNI)
  ✅ 检测到 ECH extension
  ✅ 识别出是 GREASE (特征)
  ✅ 安全封锁 (不会误伤)
  
攻击面: 大
```

#### 真实 ECH
```
DPI 可以:
  ❌ 看不到真实域名 (加密)
  ✅ 看到 cloudflare-ech.com
  ✅ 检测到 ECH extension
  ❌ 不敢封锁 (会误伤)
  
攻击面: 小
```

#### 无 ECH
```
DPI 可以:
  ✅ 看到真实域名
  ❌ 看不到 ECH extension
  ⚠️ 可以基于域名封锁
  
攻击面: 中
```

### 最佳策略

```
优先级:
  1. 真实 ECH (最安全)
  2. 无 ECH (次选)
  3. GREASE ECH (禁用)
```

---

## 实战建议

### Rust 侧实现

```rust
// 1. 尝试获取 ECH config
let ech_config = match query_ech_config(domain).await {
    Ok(config) => Some(config),
    Err(e) => {
        warn!("Failed to get ECH config: {}", e);
        None
    }
};

// 2. 配置 tunnel
let config = TunnelConfig {
    host: domain,
    port: 443,
    ech_config: ech_config,  // 有就用，没有就不用
    enforce_ech: true,       // 强制验证
    profile: Profile::Chrome133,
};

// 3. 创建连接
match create_tunnel(&config) {
    Ok(tunnel) => {
        if ech_config.is_some() {
            info!("Connected with ECH protection");
        } else {
            warn!("Connected without ECH (no config available)");
        }
    }
    Err(TlsError::EchNotAccepted) => {
        error!("ECH downgrade attack detected!");
        // 不要回退，直接失败
        return Err(e);
    }
    Err(e) => return Err(e),
}
```

### 监控指标

```rust
metrics.increment("tls_connections_total");

if ech_config.is_some() {
    if ech_accepted {
        metrics.increment("ech_success");
    } else {
        metrics.increment("ech_downgrade_detected");
    }
} else {
    metrics.increment("no_ech_available");
}
```

---

## FAQ

### Q: 为什么不用 GREASE ECH？
**A**: GREASE ECH 暴露意图（外部 SNI = 真实域名），在审查环境下增加风险。

### Q: 这样不就和真实 Chrome 不一样了？
**A**: 是的，但我们的目标是绕过审查，不是完全模拟浏览器。在审查环境下，安全性 > 完美模拟。

### Q: 如果没有 ECH config 怎么办？
**A**: 回退到普通 TLS 1.3 连接。虽然没有 ECH 保护，但至少不会暴露"想用 ECH"的意图。

### Q: 能否在 ECH 失败时回退到 GREASE？
**A**: **绝对不行**。这会暴露你的意图，让 DPI 知道你在尝试绕过审查。

### Q: cloudflare-ech.com 是什么？
**A**: Cloudflare 的 ECH 公共服务域名。所有使用 Cloudflare ECH 的连接都用这个外部 SNI，DPI 无法区分具体目标。

---

## 总结

| 策略 | 外部 SNI | ECH | 隐私 | 风险 |
|------|---------|-----|------|------|
| 真实 ECH | cloudflare-ech.com | ✅ | 高 | 低 |
| 无 ECH | example.com | ❌ | 低 | 中 |
| ~~GREASE ECH~~ | ~~example.com~~ | ~~假~~ | ~~无~~ | ~~高~~ |

**核心**: 真实 ECH 的价值不仅在于加密 SNI，更在于利用大厂域名让 DPI 投鼠忌器。
