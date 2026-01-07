const std = @import("std");
const tunnel = @import("tunnel.zig");

/// TLS 隧道配置（C ABI 兼容）
pub const TlsTunnelConfig = extern struct {
    // 服务器信息
    host: [*:0]const u8,
    port: u16,
    _padding1: [6]u8 = undefined,

    // ECH 配置（可选）
    ech_config: ?[*]const u8,
    ech_config_len: usize,
    
    // ECH 选项
    auto_ech: bool,        // 自动从 DNS 获取 ECH config
    enforce_ech: bool,     // 强制验证 ECH（防止降级攻击）
    
    // 指纹配置
    use_firefox_profile: bool,  // 使用 Firefox 120 指纹
    _padding2: [5]u8 = undefined,

    // 超时设置（毫秒）
    connect_timeout_ms: u32,
    handshake_timeout_ms: u32,
};

/// TLS 连接信息
pub const TlsInfo = extern struct {
    protocol_version: u16, // 0x0304 = TLS 1.3
    cipher_suite: u16,
    used_ech: bool,
    _padding: [3]u8 = undefined,
    server_name: [256]u8,
};

/// 错误码
pub const TlsError = enum(c_int) {
    Success = 0,
    InvalidConfig = -1,
    ConnectionFailed = -2,
    HandshakeFailed = -3,
    EchNotAccepted = -4,  // ECH 配置了但未被接受（可能是降级攻击）
    OutOfMemory = -5,
    IoError = -6,
    SslError = -7,
};

/// 不透明的隧道句柄
pub const TlsTunnel = opaque {};

// ========== 全局分配器 ==========
// 使用 C allocator 以便与 C/Rust 互操作
var gpa = std.heap.c_allocator;

// ========== 导出的 C API ==========

/// 创建 TLS 隧道
export fn tls_tunnel_create(
    config: *const TlsTunnelConfig,
    out_error: *TlsError,
) ?*TlsTunnel {
    out_error.* = TlsError.Success;

    // 转换为 Zig 配置
    const profiles = @import("profiles.zig");
    const zig_config = tunnel.TunnelConfig{
        .host = std.mem.span(config.host),
        .port = config.port,
        .ech_config = if (config.ech_config) |ptr|
            ptr[0..config.ech_config_len]
        else
            null,
        .auto_ech = config.auto_ech,
        .enforce_ech = config.enforce_ech,
        .profile = if (config.use_firefox_profile) profiles.Profile.Firefox120 else null,
        .connect_timeout_ms = config.connect_timeout_ms,
        .handshake_timeout_ms = config.handshake_timeout_ms,
    };

    // 创建隧道
    const tun = tunnel.Tunnel.create(gpa, zig_config) catch |err| {
        out_error.* = switch (err) {
            error.OutOfMemory => TlsError.OutOfMemory,
            error.ConnectionFailed => TlsError.ConnectionFailed,
            error.EchNotAccepted => TlsError.EchNotAccepted,
            else => TlsError.SslError,
        };
        return null;
    };

    // 转换为不透明指针
    return @ptrCast(tun);
}

/// 获取文件描述符
export fn tls_tunnel_get_fd(tun: *TlsTunnel) c_int {
    const tunnel_ptr: *tunnel.Tunnel = @ptrCast(@alignCast(tun));
    return tunnel_ptr.getFd();
}

/// 读取数据
export fn tls_tunnel_read(
    tun: *TlsTunnel,
    buffer: [*]u8,
    len: usize,
    out_read: *usize,
) TlsError {
    const tunnel_ptr: *tunnel.Tunnel = @ptrCast(@alignCast(tun));
    const buf = buffer[0..len];

    const n = tunnel_ptr.read(buf) catch |err| {
        return switch (err) {
            error.WouldBlock => TlsError.Success, // 非阻塞，返回 0
            else => TlsError.IoError,
        };
    };

    out_read.* = n;
    return TlsError.Success;
}

/// 写入数据
export fn tls_tunnel_write(
    tun: *TlsTunnel,
    data: [*]const u8,
    len: usize,
    out_written: *usize,
) TlsError {
    const tunnel_ptr: *tunnel.Tunnel = @ptrCast(@alignCast(tun));
    const buf = data[0..len];

    const n = tunnel_ptr.write(buf) catch |err| {
        return switch (err) {
            error.WouldBlock => TlsError.Success,
            else => TlsError.IoError,
        };
    };

    out_written.* = n;
    return TlsError.Success;
}

/// 关闭连接
export fn tls_tunnel_close(tun: *TlsTunnel) void {
    const tunnel_ptr: *tunnel.Tunnel = @ptrCast(@alignCast(tun));
    tunnel_ptr.close();
}

/// 销毁隧道
export fn tls_tunnel_destroy(tun: *TlsTunnel) void {
    const tunnel_ptr: *tunnel.Tunnel = @ptrCast(@alignCast(tun));
    tunnel_ptr.destroy();
}

/// 获取连接信息
export fn tls_tunnel_get_info(
    tun: *TlsTunnel,
    out_info: *TlsInfo,
) TlsError {
    const tunnel_ptr: *tunnel.Tunnel = @ptrCast(@alignCast(tun));

    const info = tunnel_ptr.getInfo() catch {
        return TlsError.SslError;
    };

    out_info.protocol_version = info.protocol_version;
    out_info.cipher_suite = info.cipher_suite;
    out_info.used_ech = info.used_ech;
    @memcpy(&out_info.server_name, &info.server_name);

    return TlsError.Success;
}
