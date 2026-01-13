# ech-workers-rs

Rust implementation of ECH (Encrypted Client Hello) proxy with TLS 1.3, Yamux multiplexing, and BoringSSL integration.

## ✅ Status

**ECH Integration: Complete and Working**

- ✅ DoH (DNS-over-HTTPS) for ECH config retrieval
- ✅ Zig TLS Tunnel with BoringSSL
- ✅ Chrome 120+ TLS fingerprint with ML-KEM
- ✅ End-to-end testing verified
- ✅ Yamux multiplexing

## Features

- 🔐 **ECH (Encrypted Client Hello)** - Privacy-preserving TLS extension
- 🚀 **TLS 1.3** - Via BoringSSL with ECH support
- 🌐 **Chrome Fingerprint** - Mimics Chrome 120+ TLS behavior
  - ML-KEM (X25519MLKEM768) post-quantum support
  - Full cipher suite list, ALPN, OCSP, SCT, ALPS
- 📡 **DoH Support** - Automatic ECH config retrieval
- 🔀 **Yamux Multiplexing** - Multiple streams over single connection
- 🌐 **SOCKS5 + HTTP Proxy** - Dual protocol support

## Quick Start

### Test ECH Connection

```bash
# Build
cargo build --release --example test_ech_e2e

# Test with crypto.cloudflare.com
./target/release/examples/test_ech_e2e crypto.cloudflare.com

# Test with defo.ie
./target/release/examples/test_ech_e2e defo.ie

# Use different DoH server
./target/release/examples/test_ech_e2e crypto.cloudflare.com https://dns.google/dns-query
```

Expected output:
```
✅ Got ECH config: 71 bytes
✅ TLS connection established
Protocol: 772 (TLS 1.3)
Cipher: 4865 (TLS_AES_256_GCM_SHA384)
ECH Accepted: true
✅✅✅ SUCCESS: ECH was accepted by server!
```

## Architecture

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

## Documentation

- [ECH Integration Guide](./ECH_INTEGRATION.md) - Complete integration documentation
- [ECH Security Policy](./ECH_SECURITY_POLICY.md) - Security design and policies

## Building

### Prerequisites

- Rust 1.70+
- Zig 0.11+
- CMake (for BoringSSL)

### Build Steps

```bash
# 1. Build BoringSSL (if not already built)
cd zig-tls-tunnel/vendor/boringssl
mkdir -p build && cd build
cmake -GNinja -DCMAKE_BUILD_TYPE=Release ..
ninja

# 2. Build Zig TLS Tunnel
cd ../../..
zig build -Doptimize=ReleaseFast

# 3. Build Rust project
cd ../..
cargo build --release
```

## Usage Example

```rust
use ech_workers_rs::{ech, tls};

#[tokio::main]
async fn main() -> Result<()> {
    // 1. Query ECH config via DoH
    let ech_config = ech::query_ech_config(
        "crypto.cloudflare.com",
        "https://cloudflare-dns.com/dns-query"
    ).await?;

    // 2. Create TLS config with ECH
    let config = tls::TunnelConfig::new("crypto.cloudflare.com", 443)
        .with_ech(ech_config, true);

    // 3. Connect
    let tunnel = tls::TlsTunnel::connect(config)?;

    // 4. Verify ECH
    let info = tunnel.info()?;
    assert!(info.used_ech);

    Ok(())
}
```

## Project Structure

```
src/
├── ech/                 # ECH implementation
│   ├── doh.rs          # ✅ DNS-over-HTTPS (working)
│   └── config.rs       # ECH config parsing
├── tls/                 # TLS implementation
│   ├── ffi.rs          # ✅ C FFI bindings (working)
│   └── tunnel.rs       # ✅ Safe Rust wrapper (working)
├── transport/           # Transport layer
│   ├── tls.rs          # TLS transport (for WebSocket)
│   ├── websocket.rs    # WebSocket transport
│   └── yamux.rs        # ⚠️ Yamux multiplexing (WIP)
└── proxy/               # Proxy layer (WIP)
    ├── socks5.rs       # SOCKS5 handler
    ├── http.rs         # HTTP CONNECT handler
    └── handler.rs      # Request handler

zig-tls-tunnel/          # Zig TLS module
├── src/
│   ├── api.zig         # ✅ C API exports (working)
│   ├── tunnel.zig      # TLS tunnel implementation
│   └── ssl.zig         # BoringSSL wrapper
└── vendor/boringssl/   # BoringSSL with ECH

examples/
└── test_ech_e2e.rs     # ✅ End-to-end test (working)
```

## Roadmap

### Completed ✅
- [x] DoH implementation
- [x] Zig TLS Tunnel integration
- [x] FFI bindings
- [x] ECH handshake
- [x] Chrome 120+ TLS fingerprint
- [x] ML-KEM post-quantum support
- [x] Yamux multiplexing
- [x] WebSocket transport
- [x] SOCKS5 proxy
- [x] HTTP CONNECT proxy

### Planned 📋
- [ ] Firefox fingerprint (WolfSSL)
- [ ] Brotli certificate compression
- [ ] Connection pooling

## Testing

```bash
# Unit tests
cargo test

# ECH integration test
cargo test --example test_ech_e2e

# With logging
RUST_LOG=debug cargo run --example test_ech_e2e crypto.cloudflare.com
```

## Troubleshooting

See [ECH Integration Guide](./ECH_INTEGRATION.md#故障排除) for common issues and solutions.

## License

MIT

## References

- [RFC 9460: HTTPS RR](https://datatracker.ietf.org/doc/html/rfc9460)
- [draft-ietf-tls-esni-18: ECH](https://datatracker.ietf.org/doc/html/draft-ietf-tls-esni-18)
- [BoringSSL](https://boringssl.googlesource.com/boringssl/)
- [ech-workers (Go)](https://github.com/yourusername/ech-workers)
