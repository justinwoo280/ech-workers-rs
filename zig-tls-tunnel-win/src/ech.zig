const std = @import("std");
const ssl_mod = @import("ssl.zig");

/// 配置 ECH
///
/// 使用 BoringSSL 的 SSL_set1_ech_config_list API
pub fn configure(ssl_conn: *ssl_mod.SSL, ech_config: []const u8) !void {
    try ssl_mod.setEchConfig(ssl_conn, ech_config);
    std.log.info("✅ ECH configured with {d} bytes", .{ech_config.len});
}

/// 检查 ECH 是否被服务器接受
pub fn wasAccepted(ssl_conn: *const ssl_mod.SSL) bool {
    return ssl_mod.echAccepted(ssl_conn);
}

/// 获取 ECH 覆盖的服务器名称
pub fn getNameOverride(ssl_conn: *const ssl_mod.SSL) ?[]const u8 {
    return ssl_mod.getEchNameOverride(ssl_conn);
}

/// 获取 ECH 拒绝时的 retry config
pub fn getRetryConfigs(ssl_conn: *const ssl_mod.SSL) ?[]const u8 {
    return ssl_mod.getEchRetryConfigs(ssl_conn);
}
