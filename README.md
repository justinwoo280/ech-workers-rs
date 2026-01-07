# ECH Workers RS

Rust implementation of ECH (Encrypted Client Hello) proxy client with Zig TLS Tunnel integration.

## Features

- ✅ **ECH Support**: Full Encrypted Client Hello implementation
- ✅ **DoH Integration**: DNS-over-HTTPS for ECH config queries
- ✅ **Zig TLS Tunnel**: BoringSSL-based TLS with ECH support via FFI
- ✅ **Yamux Multiplexing**: Efficient connection reuse
- ✅ **WebSocket Transport**: Compatible with standard proxy servers
- ✅ **SOCKS5 & HTTP CONNECT**: Standard proxy protocols
- ✅ **Domain Pass-through**: No local DNS resolution
- ✅ **No Fallback**: Strict ECH-only mode for security

## Architecture

```
Client App → SOCKS5/HTTP → Rust Client → DoH Query → ECH Config
                                ↓
                         Zig TLS (ECH)
                                ↓
                          WebSocket
                                ↓
                         Yamux Session
                                ↓
                          Proxy Server
```

## Project Structure

```
.
├── ech-workers-rs/          # Rust client implementation
│   ├── src/
│   │   ├── ech/            # ECH and DoH modules
│   │   ├── tls/            # Zig TLS FFI bindings
│   │   ├── transport/      # WebSocket, Yamux, connection
│   │   ├── proxy/          # SOCKS5, HTTP CONNECT, relay
│   │   └── main.rs         # Entry point
│   ├── zig-tls-tunnel/     # Zig TLS implementation (submodule)
│   └── tests/              # Integration tests
│
└── zig-tls-tunnel/          # Standalone Zig TLS module
    ├── src/                # Zig source code
    ├── vendor/boringssl/   # BoringSSL submodule
    └── build.zig           # Build configuration
```

## Building

### Prerequisites

- Rust 1.75+
- Zig 0.13.0+
- Git with submodules

### Build Steps

```bash
# Clone with submodules
git clone --recursive git@github.com:justinwoo280/ech-workers-rs.git
cd ech-workers-rs

# Build Zig TLS Tunnel
cd zig-tls-tunnel
zig build -Doptimize=ReleaseFast
cd ..

# Build Rust client
cd ech-workers-rs
cargo build --release
```

## Usage

### Basic Usage

```bash
# Start proxy client
./target/release/ech-workers-rs \
  --listen 127.0.0.1:1080 \
  --server wss://your-server.com:443 \
  --uuid your-secret-uuid \
  --doh https://cloudflare-dns.com/dns-query
```

### Configuration

| Option | Description | Default |
|--------|-------------|---------|
| `--listen` | Local SOCKS5/HTTP proxy address | `127.0.0.1:1080` |
| `--server` | Proxy server WebSocket URL | Required |
| `--uuid` | Authentication token | Required |
| `--doh` | DoH server URL | `https://cloudflare-dns.com/dns-query` |
| `--no-ech` | Disable ECH (testing only) | false |
| `--yamux` | Enable Yamux multiplexing | true |

### Testing

```bash
# Run unit tests
cargo test

# Test with curl
curl -x socks5h://127.0.0.1:1080 https://www.google.com

# Test ECH
RUST_LOG=debug ./target/release/ech-workers-rs --server wss://ech-server.com:443 --uuid xxx
```

## Security

### ECH Enforcement

This implementation enforces ECH usage when enabled:
- ECH query failure → Connection fails
- ECH not accepted by server → Connection fails
- No fallback to plain TLS

### Verification

Check logs for:
- `✅ Got ECH config: X bytes` - DoH query successful
- `✅ ECH successfully negotiated` - ECH handshake successful
- `❌ ECH not accepted` - Connection rejected (no fallback)

## Documentation

- [E2E Testing Guide](ech-workers-rs/E2E_TEST.md)
- [ECH Integration](ech-workers-rs/ECH_INTEGRATION.md)
- [Security Policy](ech-workers-rs/ECH_SECURITY_POLICY.md)
- [Implementation Status](ech-workers-rs/IMPLEMENTATION_COMPLETE.md)

## Compatibility

### Server Requirements

Compatible with Go proxy-server from [ech-workers](https://github.com/briomianopc/jarustls/tree/main/ech-workers/proxy-server):
- WebSocket + Yamux protocol
- UUID authentication
- Target address format: `host:port\n`

### Client Features

| Feature | Status |
|---------|--------|
| SOCKS5 | ✅ |
| HTTP CONNECT | ✅ |
| Domain pass-through | ✅ |
| ECH + DoH | ✅ |
| Yamux multiplexing | ✅ |
| WebSocket transport | ✅ |
| gRPC | ❌ (excluded) |

## Performance

- **Latency**: < 100ms (local network)
- **Throughput**: > 10MB/s
- **Memory**: < 50MB
- **Concurrent connections**: 100+

## License

MIT

## Credits

- Based on [ech-workers](https://github.com/briomianopc/jarustls) Go implementation
- Uses [zig-tls-tunnel](https://github.com/briomianopc/zig-tls-tunnel) for ECH support
- BoringSSL for TLS implementation
