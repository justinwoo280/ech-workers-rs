/// Safe Rust wrapper for Zig TLS tunnel

use std::ffi::CString;
use std::io::{self, Read, Write};
#[cfg(unix)]
use std::os::unix::io::{AsRawFd, RawFd};
#[cfg(windows)]
use std::os::windows::io::{AsRawSocket, RawSocket};
use tracing::{debug, info};

use super::ffi::{self, TlsError, TlsInfo, TlsTunnel as RawTlsTunnel};
use crate::error::{Error, Result};

/// TLS tunnel configuration
#[derive(Debug, Clone)]
pub struct TunnelConfig {
    pub host: String,
    pub port: u16,
    pub ech_config: Option<Vec<u8>>,
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
            enforce_ech: true,
            use_firefox_profile: false, // 使用 BoringSSL 默认指纹 + GREASE
            connect_timeout_ms: 10000,
            handshake_timeout_ms: 10000,
        }
    }
}

impl TunnelConfig {
    /// Create new config with host and port
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
            ..Default::default()
        }
    }
    
    /// Set ECH configuration
    pub fn with_ech(mut self, ech_config: Vec<u8>, enforce: bool) -> Self {
        self.ech_config = Some(ech_config);
        self.enforce_ech = enforce;
        self
    }
}

/// Safe TLS tunnel wrapper
pub struct TlsTunnel {
    inner: *mut RawTlsTunnel,
    _host: CString,  // Keep alive for FFI
}

impl TlsTunnel {
    /// Quick connect with minimal config
    /// 
    /// # Example
    /// ```
    /// let tunnel = TlsTunnel::new("example.com", 443, Some(&ech_config))?;
    /// ```
    pub fn new(host: &str, port: u16, ech_config: Option<&[u8]>) -> Result<Self> {
        let config = TunnelConfig {
            host: host.to_string(),
            port,
            ech_config: ech_config.map(|c| c.to_vec()),
            enforce_ech: ech_config.is_some(),
            use_firefox_profile: false, // 使用 BoringSSL 默认指纹
            connect_timeout_ms: 5000,
            handshake_timeout_ms: 5000,
        };
        Self::connect(config)
    }
    
    /// Create a new TLS tunnel connection with full config
    pub fn connect(config: TunnelConfig) -> Result<Self> {
        debug!("Connecting to {}:{}", config.host, config.port);
        
        let host_cstr = CString::new(config.host.clone())
            .map_err(|_| Error::InvalidConfig("Invalid host".into()))?;
        
        let c_config = ffi::TlsTunnelConfig {
            host: host_cstr.as_ptr(),
            port: config.port,
            _padding1: [0; 6],
            ech_config: config.ech_config.as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null()),
            ech_config_len: config.ech_config.as_ref()
                .map(|v| v.len())
                .unwrap_or(0),
            enforce_ech: config.enforce_ech,
            _padding_ech: false,
            use_firefox_profile: config.use_firefox_profile,
            _padding2: [0; 5],
            connect_timeout_ms: config.connect_timeout_ms,
            handshake_timeout_ms: config.handshake_timeout_ms,
        };
        
        let mut error = TlsError::Success;
        let tunnel = unsafe {
            ffi::tls_tunnel_create(&c_config, &mut error)
        };
        
        if tunnel.is_null() {
            return Err(match error {
                TlsError::InvalidConfig => Error::InvalidConfig("TLS config invalid".into()),
                TlsError::ConnectionFailed => Error::ConnectionFailed,
                TlsError::HandshakeFailed => Error::TlsHandshakeFailed,
                TlsError::EchNotAccepted => Error::EchNotAccepted,
                TlsError::OutOfMemory => Error::OutOfMemory,
                _ => Error::Tls(format!("{:?}", error)),
            });
        }
        
        info!("TLS tunnel established to {}:{}", config.host, config.port);
        
        Ok(Self {
            inner: tunnel,
            _host: host_cstr,
        })
    }
    
    /// Get connection information
    pub fn info(&self) -> Result<ConnectionInfo> {
        let mut info = TlsInfo {
            protocol_version: 0,
            cipher_suite: 0,
            used_ech: false,
            _padding: [0; 3],
            server_name: [0; 256],
        };
        
        let error = unsafe {
            ffi::tls_tunnel_get_info(self.inner, &mut info)
        };
        
        if error != TlsError::Success {
            return Err(Error::Tls(format!("Failed to get info: {:?}", error)));
        }
        
        Ok(ConnectionInfo {
            protocol_version: info.protocol_version,
            cipher_suite: info.cipher_suite,
            used_ech: info.used_ech,
        })
    }
    
    /// Check if ECH was accepted by the server
    #[inline]
    pub fn ech_accepted(&self) -> bool {
        self.info().map(|i| i.used_ech).unwrap_or(false)
    }
    
    /// Get the raw file descriptor (for poll/select)
    #[inline]
    pub fn raw_fd(&self) -> i32 {
        unsafe { ffi::tls_tunnel_get_fd(self.inner) }
    }
}

impl Read for TlsTunnel {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let mut read_bytes = 0;
        let error = unsafe {
            ffi::tls_tunnel_read(
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
            ffi::tls_tunnel_write(
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

#[cfg(unix)]
impl AsRawFd for TlsTunnel {
    fn as_raw_fd(&self) -> RawFd {
        unsafe { ffi::tls_tunnel_get_fd(self.inner) as RawFd }
    }
}

#[cfg(windows)]
impl AsRawSocket for TlsTunnel {
    fn as_raw_socket(&self) -> RawSocket {
        unsafe { ffi::tls_tunnel_get_fd(self.inner) as RawSocket }
    }
}

impl Drop for TlsTunnel {
    fn drop(&mut self) {
        unsafe {
            ffi::tls_tunnel_close(self.inner);
            ffi::tls_tunnel_destroy(self.inner);
        }
    }
}

unsafe impl Send for TlsTunnel {}
unsafe impl Sync for TlsTunnel {}

// 实现 tokio::io traits
impl tokio::io::AsyncRead for TlsTunnel {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // 同步读取（Zig TLS tunnel 是同步的）
        let unfilled = buf.initialize_unfilled();
        match self.read(unfilled) {
            Ok(n) => {
                buf.advance(n);
                std::task::Poll::Ready(Ok(()))
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready(Err(e)),
        }
    }
}

impl tokio::io::AsyncWrite for TlsTunnel {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        // 同步写入
        match self.write(buf) {
            Ok(n) => std::task::Poll::Ready(Ok(n)),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready(Err(e)),
        }
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match self.flush() {
            Ok(()) => std::task::Poll::Ready(Ok(())),
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::task::Poll::Pending
            }
            Err(e) => std::task::Poll::Ready(Err(e)),
        }
    }

    fn poll_shutdown(
        self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        // TLS 连接关闭
        std::task::Poll::Ready(Ok(()))
    }
}

impl Unpin for TlsTunnel {}

/// Connection information
#[derive(Debug, Clone)]
pub struct ConnectionInfo {
    pub protocol_version: u16,
    pub cipher_suite: u16,
    pub used_ech: bool,
}

impl ConnectionInfo {
    pub fn protocol_name(&self) -> &'static str {
        match self.protocol_version {
            0x0301 => "TLS 1.0",
            0x0302 => "TLS 1.1",
            0x0303 => "TLS 1.2",
            0x0304 => "TLS 1.3",
            _ => "Unknown",
        }
    }
    
    pub fn cipher_name(&self) -> &'static str {
        match self.cipher_suite {
            0x1301 => "TLS_AES_128_GCM_SHA256",
            0x1302 => "TLS_AES_256_GCM_SHA384",
            0x1303 => "TLS_CHACHA20_POLY1305_SHA256",
            _ => "Unknown",
        }
    }
}
