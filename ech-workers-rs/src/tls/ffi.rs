/// Zig TLS Tunnel FFI bindings
///
/// C API bindings for the Zig TLS tunnel module

use std::os::raw::{c_char, c_int, c_uint, c_ushort};

// ========== Types ==========

/// TLS tunnel configuration (C ABI compatible)
#[repr(C)]
pub struct TlsTunnelConfig {
    pub host: *const c_char,          // SNI 主机名
    pub port: c_ushort,
    pub _padding1: [u8; 6],
    
    // 连接目标（可选，用于绕过 DNS）
    pub connect_host: *const c_char,  // 实际连接的主机，null 表示使用 host
    pub _padding_connect: [u8; 8],
    
    pub ech_config: *const u8,
    pub ech_config_len: usize,
    
    pub enforce_ech: bool,
    pub _padding_ech: bool,  // ABI 兼容
    pub use_firefox_profile: bool,
    pub _padding2: [u8; 5],
    
    pub connect_timeout_ms: c_uint,
    pub handshake_timeout_ms: c_uint,
}

/// TLS connection information
#[repr(C)]
pub struct TlsInfo {
    pub protocol_version: c_ushort,
    pub cipher_suite: c_ushort,
    pub used_ech: bool,
    pub _padding: [u8; 3],
    pub server_name: [u8; 256],
}

/// TLS error codes
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

/// Opaque TLS tunnel handle
#[repr(C)]
pub struct TlsTunnel {
    _private: [u8; 0],
}

// ========== C API Functions ==========

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
