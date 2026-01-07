# Rust FFI Integration Guide

## Overview

This guide shows how to integrate the Zig TLS tunnel module into your Rust project.

---

## Architecture

```
Rust Application
    ↓
Rust FFI Bindings (bindgen)
    ↓
Zig TLS Tunnel (C API)
    ↓
BoringSSL (static library)
```

---

## Step 1: Build Zig Module

```bash
cd zig-tls-tunnel
zig build

# Output:
# - zig-out/lib/libzig-tls-tunnel.a (~9KB - only Zig code)
# - vendor/boringssl/build/libssl.a (~31MB)
# - vendor/boringssl/build/libcrypto.a (~32MB)
```

---

## Step 2: Create Rust Bindings

### Option A: Manual Bindings (Recommended)

Create `src/ffi.rs`:

```rust
use std::os::raw::{c_char, c_int, c_uint, c_ushort};

// TLS 隧道配置
#[repr(C)]
pub struct TlsTunnelConfig {
    pub host: *const c_char,
    pub port: c_ushort,
    _padding1: [u8; 6],
    
    pub ech_config: *const u8,
    pub ech_config_len: usize,
    
    pub auto_ech: bool,
    pub enforce_ech: bool,
    pub use_firefox_profile: bool,
    _padding2: [u8; 5],
    
    pub connect_timeout_ms: c_uint,
    pub handshake_timeout_ms: c_uint,
}

// TLS 连接信息
#[repr(C)]
pub struct TlsInfo {
    pub protocol_version: c_ushort,
    pub cipher_suite: c_ushort,
    pub used_ech: bool,
    _padding: [u8; 3],
    pub server_name: [u8; 256],
}

// 错误码
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TlsError {
    Success = 0,
    InvalidConfig = -1,
    ConnectionFailed = -2,
    HandshakeFailed = -3,
    EchNotAccepted = -4,
    OutOfMemory = -5,
    IoError = -6,
    SslError = -7,
}

// 不透明句柄
#[repr(C)]
pub struct TlsTunnel {
    _private: [u8; 0],
}

// C API 声明
extern "C" {
    pub fn tls_tunnel_create(
        config: *const TlsTunnelConfig,
        out_error: *mut TlsError,
    ) -> *mut TlsTunnel;
    
    pub fn tls_tunnel_get_fd(tunnel: *mut TlsTunnel) -> c_int;
    
    pub fn tls_tunnel_read(
        tunnel: *mut TlsTunnel,
        buffer: *mut u8,
        len: usize,
        out_read: *mut usize,
    ) -> TlsError;
    
    pub fn tls_tunnel_write(
        tunnel: *mut TlsTunnel,
        data: *const u8,
        len: usize,
        out_written: *mut usize,
    ) -> TlsError;
    
    pub fn tls_tunnel_close(tunnel: *mut TlsTunnel);
    
    pub fn tls_tunnel_destroy(tunnel: *mut TlsTunnel);
    
    pub fn tls_tunnel_get_info(
        tunnel: *mut TlsTunnel,
        out_info: *mut TlsInfo,
    ) -> TlsError;
}
```

### Option B: Using bindgen (Advanced)

```toml
# build.rs dependencies
[build-dependencies]
bindgen = "0.69"
```

---

## Step 3: Create Safe Rust Wrapper

Create `src/tunnel.rs`:

```rust
use std::ffi::CString;
use std::io::{self, Read, Write};
use crate::ffi::*;

pub struct TlsTunnel {
    inner: *mut ffi::TlsTunnel,
}

impl TlsTunnel {
    pub fn connect(config: TunnelConfig) -> Result<Self, TlsError> {
        let host_cstr = CString::new(config.host)?;
        
        let c_config = TlsTunnelConfig {
            host: host_cstr.as_ptr(),
            port: config.port,
            _padding1: [0; 6],
            ech_config: config.ech_config.as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null()),
            ech_config_len: config.ech_config.as_ref()
                .map(|v| v.len())
                .unwrap_or(0),
            auto_ech: config.auto_ech,
            enforce_ech: config.enforce_ech,
            use_firefox_profile: config.use_firefox_profile,
            _padding2: [0; 5],
            connect_timeout_ms: config.connect_timeout_ms,
            handshake_timeout_ms: config.handshake_timeout_ms,
        };
        
        let mut error = TlsError::Success;
        let tunnel = unsafe {
            tls_tunnel_create(&c_config, &mut error)
        };
        
        if tunnel.is_null() {
            return Err(error);
        }
        
        Ok(Self { inner: tunnel })
    }
    
    pub fn info(&self) -> Result<TlsInfo, TlsError> {
        let mut info = TlsInfo {
            protocol_version: 0,
            cipher_suite: 0,
            used_ech: false,
            _padding: [0; 3],
            server_name: [0; 256],
        };
        
        let error = unsafe {
            tls_tunnel_get_info(self.inner, &mut info)
        };
        
        if error != TlsError::Success {
            return Err(error);
        }
        
        Ok(info)
    }
    
    pub fn as_raw_fd(&self) -> i32 {
        unsafe { tls_tunnel_get_fd(self.inner) }
    }
}

impl Read for TlsTunnel {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_bytes = 0;
        let error = unsafe {
            tls_tunnel_read(
                self.inner,
                buf.as_mut_ptr(),
                buf.len(),
                &mut read_bytes,
            )
        };
        
        if error != TlsError::Success {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("TLS read error: {:?}", error),
            ));
        }
        
        Ok(read_bytes)
    }
}

impl Write for TlsTunnel {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let mut written = 0;
        let error = unsafe {
            tls_tunnel_write(
                self.inner,
                buf.as_ptr(),
                buf.len(),
                &mut written,
            )
        };
        
        if error != TlsError::Success {
            return Err(io::Error::new(
                io::ErrorKind::Other,
                format!("TLS write error: {:?}", error),
            ));
        }
        
        Ok(written)
    }
    
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Drop for TlsTunnel {
    fn drop(&mut self) {
        unsafe {
            tls_tunnel_close(self.inner);
            tls_tunnel_destroy(self.inner);
        }
    }
}

unsafe impl Send for TlsTunnel {}
unsafe impl Sync for TlsTunnel {}

// Configuration
pub struct TunnelConfig {
    pub host: String,
    pub port: u16,
    pub ech_config: Option<Vec<u8>>,
    pub auto_ech: bool,
    pub enforce_ech: bool,
    pub use_firefox_profile: bool,
    pub connect_timeout_ms: u32,
    pub handshake_timeout_ms: u32,
}

impl Default for TunnelConfig {
    fn default() -> Self {
        Self {
            host: String::new(),
            port: 443,
            ech_config: None,
            auto_ech: false,  // Rust handles DNS
            enforce_ech: true,
            use_firefox_profile: true,
            connect_timeout_ms: 10000,
            handshake_timeout_ms: 10000,
        }
    }
}
```

---

## Step 4: Configure Cargo Build

### Cargo.toml

```toml
[package]
name = "your-project"
version = "0.1.0"
edition = "2021"

[dependencies]
# Your dependencies

[build-dependencies]
# If using bindgen
# bindgen = "0.69"
```

### build.rs

```rust
use std::path::PathBuf;

fn main() {
    let zig_lib_path = PathBuf::from("zig-tls-tunnel/zig-out/lib");
    let boringssl_path = PathBuf::from("zig-tls-tunnel/vendor/boringssl/build");
    
    // Link Zig library (~9KB)
    println!("cargo:rustc-link-search=native={}", zig_lib_path.display());
    println!("cargo:rustc-link-lib=static=zig-tls-tunnel");
    
    // Link BoringSSL (~63MB total)
    println!("cargo:rustc-link-search=native={}", boringssl_path.display());
    println!("cargo:rustc-link-lib=static=ssl");
    println!("cargo:rustc-link-lib=static=crypto");
    
    // Link C++ (BoringSSL needs it)
    println!("cargo:rustc-link-lib=dylib=stdc++");
    
    // Rerun if libraries change
    println!("cargo:rerun-if-changed=zig-tls-tunnel/zig-out/lib/libzig-tls-tunnel.a");
    println!("cargo:rerun-if-changed=zig-tls-tunnel/vendor/boringssl/build/libssl.a");
}
```

---

## Step 5: Usage Example

```rust
use your_project::tunnel::{TlsTunnel, TunnelConfig};
use std::io::{Read, Write};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 1. Get ECH config from DNS (Rust handles this)
    let ech_config = query_ech_config("example.com").await?;
    
    // 2. Configure tunnel
    let config = TunnelConfig {
        host: "example.com".to_string(),
        port: 443,
        ech_config: Some(ech_config),
        auto_ech: false,  // We already got ECH config
        enforce_ech: true,  // Fail if ECH not accepted
        use_firefox_profile: true,
        ..Default::default()
    };
    
    // 3. Create TLS tunnel
    let mut tunnel = TlsTunnel::connect(config)?;
    
    // 4. Check connection info
    let info = tunnel.info()?;
    println!("TLS Version: 0x{:04x}", info.protocol_version);
    println!("Cipher: 0x{:04x}", info.cipher_suite);
    println!("ECH Accepted: {}", info.used_ech);
    
    // 5. Use as normal Read/Write
    tunnel.write_all(b"GET / HTTP/1.1\r\nHost: example.com\r\n\r\n")?;
    
    let mut response = Vec::new();
    tunnel.read_to_end(&mut response)?;
    
    println!("Response: {}", String::from_utf8_lossy(&response));
    
    Ok(())
}

// DNS query helper (implement with trust-dns or similar)
async fn query_ech_config(domain: &str) -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Query DNS HTTPS RR
    // Extract ECH config
    // Return as Vec<u8>
    todo!("Implement DNS HTTPS RR query")
}
```

---

## Error Handling

```rust
match TlsTunnel::connect(config) {
    Ok(tunnel) => {
        println!("Connected successfully");
    }
    Err(TlsError::EchNotAccepted) => {
        eprintln!("ECH downgrade attack detected!");
        // Don't fallback - this is a security issue
    }
    Err(TlsError::ConnectionFailed) => {
        eprintln!("Connection failed");
    }
    Err(e) => {
        eprintln!("TLS error: {:?}", e);
    }
}
```

---

## Testing

```rust
#[cfg(test)]
mod tests {
    use super::*;
    
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
        
        assert_eq!(info.protocol_version, 0x0304); // TLS 1.3
    }
}
```

---

## Build Commands

```bash
# Build Zig module first
cd zig-tls-tunnel
zig build

# Then build Rust project
cd ..
cargo build --release
```

---

## Troubleshooting

### Linking Errors

If you get undefined symbol errors:

```bash
# Check symbols in Zig library
nm zig-tls-tunnel/zig-out/lib/libzig-tls-tunnel.a | grep tls_tunnel

# Make sure BoringSSL is built
ls zig-tls-tunnel/vendor/boringssl/build/libssl.a
```

### Runtime Errors

Enable logging:

```rust
env_logger::init();
log::info!("Connecting to {}", config.host);
```

---

## Performance Tips

1. **Reuse connections** when possible
2. **Use async I/O** with tokio or async-std
3. **Pool connections** for high throughput
4. **Monitor ECH acceptance rate** to detect attacks

---

## Security Checklist

- ✅ Always set `enforce_ech = true` when using ECH
- ✅ Never fallback to non-ECH on `EchNotAccepted` error
- ✅ Validate ECH config from DNS (DNSSEC)
- ✅ Monitor for downgrade attacks
- ✅ Use Firefox profile (no GREASE ECH)

---

## Next Steps

1. Implement DNS HTTPS RR query in Rust
2. Add connection pooling
3. Add metrics and monitoring
4. Integrate with your HTTP client

---

## Reference

- Zig module: `zig-tls-tunnel/`
- C API: `src/api.zig`
- Examples: `examples/`
- Documentation: `*.md` files
