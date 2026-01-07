const std = @import("std");

// 导出子模块
pub const api = @import("api.zig");
pub const tunnel = @import("tunnel.zig");
pub const ssl = @import("ssl.zig");
pub const ech = @import("ech.zig");
pub const dns = @import("dns.zig");
pub const profiles = @import("profiles.zig");

// 导出 C API（供 Rust FFI 使用）
pub const tls_tunnel_create = api.tls_tunnel_create;
pub const tls_tunnel_get_fd = api.tls_tunnel_get_fd;
pub const tls_tunnel_read = api.tls_tunnel_read;
pub const tls_tunnel_write = api.tls_tunnel_write;
pub const tls_tunnel_close = api.tls_tunnel_close;
pub const tls_tunnel_destroy = api.tls_tunnel_destroy;
pub const tls_tunnel_get_info = api.tls_tunnel_get_info;

test "basic functionality" {
    const testing = std.testing;
    try testing.expect(true);
}
