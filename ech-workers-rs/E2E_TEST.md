# 端到端测试指南

## 测试环境

### 1. 启动 Go 服务端

```bash
cd /workspaces/jarustls/ech-workers/proxy-server
go run main.go --port 8080
```

服务端配置：
- 端口：8080
- UUID：`d342d11e-d424-4583-b36e-524ab1f0afa4`（默认）
- 模式：WebSocket + Yamux

### 2. 构建 Rust 客户端

```bash
cd /workspaces/jarustls/ech-workers-rs
cargo build --release
```

### 3. 运行客户端

```bash
# 基本测试（不使用 ECH）
./target/release/ech-workers-rs \
  --listen 127.0.0.1:1080 \
  --server ws://127.0.0.1:8080 \
  --uuid d342d11e-d424-4583-b36e-524ab1f0afa4 \
  --no-ech

# ECH 测试（需要支持 ECH 的服务器）
./target/release/ech-workers-rs \
  --listen 127.0.0.1:1080 \
  --server wss://your-ech-server.com:443 \
  --uuid your-uuid \
  --doh https://cloudflare-dns.com/dns-query
```

## 测试场景

### 场景 1：SOCKS5 基本连接

```bash
# 启动客户端
./target/release/ech-workers-rs --listen 127.0.0.1:1080 --server ws://127.0.0.1:8080 --no-ech

# 测试 HTTP
curl -x socks5h://127.0.0.1:1080 http://example.com

# 测试 HTTPS
curl -x socks5h://127.0.0.1:1080 https://www.google.com
```

### 场景 2：HTTP CONNECT

```bash
# 启动客户端（HTTP 代理模式）
./target/release/ech-workers-rs --listen 127.0.0.1:8888 --server ws://127.0.0.1:8080 --no-ech --http

# 测试
curl -x http://127.0.0.1:8888 https://www.google.com
```

### 场景 3：域名透传

```bash
# 使用 SOCKS5h（域名透传）
curl -x socks5h://127.0.0.1:1080 https://www.cloudflare.com

# 验证日志中显示域名而非 IP
```

### 场景 4：Yamux 多路复用

```bash
# 并发测试
for i in {1..10}; do
  curl -x socks5h://127.0.0.1:1080 https://www.google.com &
done
wait

# 验证日志中显示复用同一个 Yamux 会话
```

### 场景 5：ECH 端到端（需要真实服务器）

```bash
# 部署 Go 服务端到支持 ECH 的域名
# 例如：your-server.com（已配置 HTTPS 记录）

# 运行客户端
./target/release/ech-workers-rs \
  --listen 127.0.0.1:1080 \
  --server wss://your-server.com:443 \
  --uuid your-uuid \
  --doh https://cloudflare-dns.com/dns-query

# 测试
curl -x socks5h://127.0.0.1:1080 https://www.google.com

# 验证日志中显示 "✅ ECH successfully negotiated"
```

## 验证清单

### 功能验证

- [ ] SOCKS5 握手成功
- [ ] HTTP CONNECT 握手成功
- [ ] 域名透传（不做本地 DNS 解析）
- [ ] 数据正确转发（HTTP/HTTPS）
- [ ] Yamux 会话复用
- [ ] 并发连接处理
- [ ] 错误处理（无效地址、超时）

### ECH 验证

- [ ] DoH 查询成功
- [ ] ECH 配置解析正确
- [ ] TLS 握手使用 ECH
- [ ] 服务端接受 ECH
- [ ] 无 fallback 到普通 TLS

### 性能验证

- [ ] 延迟 < 100ms（本地）
- [ ] 吞吐量 > 10MB/s
- [ ] 内存占用 < 50MB
- [ ] CPU 占用 < 10%

## 调试技巧

### 启用详细日志

```bash
RUST_LOG=debug ./target/release/ech-workers-rs ...
```

### 抓包分析

```bash
# 抓取 WebSocket 流量
tcpdump -i lo -w /tmp/ws.pcap port 8080

# 分析
wireshark /tmp/ws.pcap
```

### 检查 Yamux 状态

查看日志中的：
- `✅ Opened stream on existing session` - 复用成功
- `Creating new Yamux session` - 新建会话

### 检查 ECH 状态

查看日志中的：
- `✅ Got ECH config: X bytes` - DoH 查询成功
- `✅ ECH successfully negotiated` - ECH 握手成功
- `❌ ECH not accepted` - ECH 失败（应该直接报错）

## 常见问题

### 1. 连接超时

检查：
- Go 服务端是否运行
- 端口是否正确
- 防火墙设置

### 2. ECH 查询失败

检查：
- DoH 服务器是否可达
- 目标域名是否有 HTTPS 记录
- 网络是否支持 DoH

### 3. Yamux 错误

检查：
- WebSocket 连接是否稳定
- 服务端是否支持 Yamux
- 版本是否兼容

### 4. 数据转发错误

检查：
- 目标地址是否正确
- 服务端是否能访问目标
- 网络是否稳定

## 性能测试

### 吞吐量测试

```bash
# 下载大文件
curl -x socks5h://127.0.0.1:1080 https://speed.cloudflare.com/100mb.bin -o /dev/null

# 使用 iperf3（需要服务端支持）
iperf3 -c target-server -p 5201 --socks5 127.0.0.1:1080
```

### 并发测试

```bash
# Apache Bench
ab -n 1000 -c 10 -X 127.0.0.1:1080 http://example.com/

# wrk
wrk -t 4 -c 100 -d 30s --proxy socks5://127.0.0.1:1080 http://example.com/
```

### 延迟测试

```bash
# 多次测试取平均值
for i in {1..10}; do
  time curl -x socks5h://127.0.0.1:1080 https://www.google.com -o /dev/null -s
done
```

## 与 Go 版本对比

| 功能 | Go 版本 | Rust 版本 | 状态 |
|------|---------|-----------|------|
| SOCKS5 | ✅ | ✅ | 完成 |
| HTTP CONNECT | ✅ | ✅ | 完成 |
| WebSocket | ✅ | ✅ | 完成 |
| Yamux | ✅ | ✅ | 完成 |
| ECH | ✅ | ✅ | 完成 |
| DoH | ✅ | ✅ | 完成 |
| gRPC | ✅ | ❌ | 排除 |
| Web UI | ✅ | ❌ | 未实现 |

## 下一步

1. 部署 Go 服务端到生产环境
2. 配置 ECH 支持的域名
3. 运行完整的端到端测试
4. 性能调优和压力测试
5. 生产环境监控
