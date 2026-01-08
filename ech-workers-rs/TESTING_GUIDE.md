## 测试指南

### 前置条件

1. **Go 服务端运行**
```bash
cd /workspaces/jarustls/ech-workers/proxy-server
go run main.go -listen :8443 -cert cert.pem -key key.pem
```

2. **Rust 客户端编译**
```bash
cd /workspaces/jarustls/ech-workers-rs
cargo build --release
```

### 测试场景

#### 1. ECH + TLS 连接测试

```bash
# 测试 DoH 查询
./target/release/ech-workers-rs test-doh crypto.cloudflare.com

# 测试 ECH 连接
./target/release/ech-workers-rs connect crypto.cloudflare.com
```

#### 2. SOCKS5 代理测试

```bash
# 启动代理
./target/release/ech-workers-rs proxy \
  --listen 127.0.0.1:1080 \
  --server example.com:8443/ws \
  --token mytoken \
  --ech \
  --yamux

# 测试 SOCKS5
curl --socks5 127.0.0.1:1080 https://www.google.com
curl --socks5 127.0.0.1:1080 https://www.cloudflare.com

# 使用 proxychains
proxychains4 curl https://ipinfo.io
```

#### 3. HTTP CONNECT 代理测试

```bash
# 测试 HTTP CONNECT
curl --proxy 127.0.0.1:1080 https://www.google.com
curl --proxy 127.0.0.1:1080 https://api.github.com

# 使用环境变量
export https_proxy=http://127.0.0.1:1080
curl https://www.google.com
```

#### 4. Yamux vs 简单模式对比

```bash
# Yamux 模式（多路复用）
./target/release/ech-workers-rs proxy \
  --listen 127.0.0.1:1080 \
  --server example.com:8443/ws \
  --token mytoken \
  --yamux

# 简单模式（每个连接一个 WebSocket）
./target/release/ech-workers-rs proxy \
  --listen 127.0.0.1:1080 \
  --server example.com:8443/ws \
  --token mytoken \
  --no-yamux
```

### 调试

#### 启用详细日志

```bash
# Debug 级别
./target/release/ech-workers-rs proxy --verbose ...

# Trace 级别
RUST_LOG=trace ./target/release/ech-workers-rs proxy ...

# 特定模块
RUST_LOG=ech_workers_rs::transport=trace ./target/release/ech-workers-rs proxy ...
```

#### 抓包分析

```bash
# 抓取本地代理流量
sudo tcpdump -i lo -w proxy.pcap port 1080

# 抓取到服务器的流量
sudo tcpdump -i any -w server.pcap host example.com and port 8443
```

### 性能测试

#### 吞吐量测试

```bash
# 下载大文件
time curl --socks5 127.0.0.1:1080 https://speed.cloudflare.com/100mb

# 并发测试
for i in {1..10}; do
  curl --socks5 127.0.0.1:1080 https://www.google.com &
done
wait
```

#### 延迟测试

```bash
# 测试延迟
for i in {1..10}; do
  time curl --socks5 127.0.0.1:1080 https://www.google.com > /dev/null
done
```

### 故障排查

#### 常见问题

1. **ECH 查询失败**
```
Error: DNS query failed: No ECH config found
```
解决：检查域名是否支持 ECH，尝试不同的 DoH 服务器

2. **连接超时**
```
Error: Connection timeout
```
解决：检查服务器地址、防火墙规则、网络连接

3. **Yamux 会话失败**
```
Error: Yamux connection closed
```
解决：检查 WebSocket 连接是否稳定，查看服务端日志

4. **SOCKS5 握手失败**
```
Error: Invalid SOCKS version
```
解决：确认客户端使用 SOCKS5 协议，检查字节序

### 单元测试

```bash
# 运行所有测试
cargo test

# 运行特定模块测试
cargo test socks5
cargo test http
cargo test relay

# 带日志的测试
RUST_LOG=debug cargo test -- --nocapture
```

### 集成测试

```bash
# 端到端测试
cargo test --test integration -- --nocapture

# 性能基准测试
cargo bench
```

### 监控

#### 连接统计

```bash
# 查看活跃连接
netstat -an | grep 1080

# 查看 Yamux 会话
# (需要添加 metrics 端点)
curl http://127.0.0.1:9090/metrics
```

#### 日志分析

```bash
# 统计连接数
grep "New connection" proxy.log | wc -l

# 统计错误
grep "ERROR" proxy.log | sort | uniq -c

# 分析延迟
grep "Relay finished" proxy.log | awk '{print $NF}'
```

### 与 Go 版本对比

| 功能 | Go 版本 | Rust 版本 | 状态 |
|------|---------|-----------|------|
| ECH 支持 | ✅ | ✅ | 完成 |
| DoH 查询 | ✅ | ✅ | 完成 |
| SOCKS5 | ✅ | ✅ | 完成 |
| HTTP CONNECT | ✅ | ✅ | 完成 |
| Yamux | ✅ | ✅ | 完成 |
| gRPC | ✅ | ❌ | 不实现 |
| 性能 | 基准 | 待测试 | - |

### 下一步

1. ✅ 基本功能测试
2. ⚠️ 与 Go 服务端兼容性测试
3. ⚠️ 性能对比测试
4. ⚠️ 稳定性测试（长时间运行）
5. ⚠️ 错误恢复测试
