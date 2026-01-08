const std = @import("std");
const ssl = @import("ssl.zig");
const ech = @import("ech.zig");
const dns = @import("dns.zig");
const profiles = @import("profiles.zig");

pub const TunnelConfig = struct {
    host: []const u8,
    port: u16,
    ech_config: ?[]const u8 = null,
    auto_ech: bool = false,  // Rust 侧通过 DoH 处理 ECH 查询
    
    // ECH enforcement: fail if ECH config provided but not accepted
    // This prevents downgrade attacks where DPI strips ECH
    // CRITICAL: No fallback to GREASE ECH - either real ECH or nothing
    enforce_ech: bool = true,
    
    // Browser fingerprint profile (Firefox 120)
    profile: ?profiles.Profile = null, // null = no fingerprint modification
    
    connect_timeout_ms: u32 = 10000,
    handshake_timeout_ms: u32 = 10000,
};

pub const TunnelInfo = struct {
    protocol_version: u16,
    cipher_suite: u16,
    used_ech: bool,
    server_name: [256]u8,
};

pub const Tunnel = struct {
    allocator: std.mem.Allocator,
    socket: std.posix.socket_t,
    ssl_ctx: *ssl.SSL_CTX,
    ssl_conn: *ssl.SSL,
    config: TunnelConfig,
    host_z: [:0]u8, // null-terminated hostname

    pub fn create(
        allocator: std.mem.Allocator,
        config: TunnelConfig,
    ) !*Tunnel {
        var self = try allocator.create(Tunnel);
        errdefer allocator.destroy(self);

        self.allocator = allocator;
        self.config = config;

        // 复制 hostname 并添加 null terminator
        self.host_z = try allocator.dupeZ(u8, config.host);
        errdefer allocator.free(self.host_z);

        // 初始化 SSL
        try ssl.init();

        // 创建 SSL context
        self.ssl_ctx = try ssl.createContext();
        errdefer ssl.destroyContext(self.ssl_ctx);

        // 配置 TLS 1.3 only
        try ssl.setTls13Only(self.ssl_ctx);

        // 设置默认证书验证路径
        try ssl.setDefaultVerifyPaths(self.ssl_ctx);

        // 建立 TCP 连接
        self.socket = try connectTcp(
            allocator,
            config.host,
            config.port,
            config.connect_timeout_ms,
        );
        errdefer std.posix.close(self.socket);

        // 创建 SSL 对象
        self.ssl_conn = try ssl.createSsl(self.ssl_ctx);
        errdefer ssl.destroySsl(self.ssl_conn);

        // 绑定 socket
        try ssl.setFd(self.ssl_conn, self.socket);

        // 设置 SNI
        try ssl.setHostname(self.ssl_conn, self.host_z.ptr);
        
        // Check if we have real ECH config
        var has_real_ech = false;
        var ech_record: ?dns.HttpsRecord = null;
        defer if (ech_record) |*rec| rec.deinit(allocator);
        
        if (config.ech_config) |_| {
            has_real_ech = true;
        } else if (config.auto_ech) {
            // Try to get ECH config from DNS
            ech_record = try dns.queryHttpsRecord(allocator, config.host);
            if (ech_record) |rec| {
                if (rec.ech_config) |_| {
                    has_real_ech = true;
                }
            }
        }
        
        // Apply browser fingerprint profile (Firefox 120)
        if (config.profile) |prof| {
            try prof.apply(self.ssl_ctx, self.ssl_conn, has_real_ech);
        }

        // Configure real ECH if available
        var ech_configured = false;
        if (config.ech_config) |ech_cfg| {
            try ech.configure(self.ssl_conn, ech_cfg);
            ech_configured = true;
        } else if (ech_record) |rec| {
            if (rec.ech_config) |ech_cfg| {
                std.log.info("Found ECH config via DNS HTTPS RR for {s}", .{config.host});
                try ech.configure(self.ssl_conn, ech_cfg);
                ech_configured = true;
            } else {
                std.log.info("HTTPS RR found but no ECH config for {s}", .{config.host});
            }
        } else if (config.auto_ech) {
            std.log.info("No HTTPS RR found for {s}, ECH not available", .{config.host});
        }

        // 执行 TLS 握手
        try performHandshake(self.ssl_conn, config.handshake_timeout_ms);

        // CRITICAL: Check if ECH was accepted (防止降级攻击)
        if (ech_configured and config.enforce_ech) {
            const ech_accepted = ech.wasAccepted(self.ssl_conn);
            if (!ech_accepted) {
                // ECH was configured but NOT accepted by server
                // This could be:
                // 1. DPI/Firewall stripped ECH extension (ATTACK!)
                // 2. Server doesn't support ECH (misconfiguration)
                // 3. ECH config is invalid/expired
                std.log.err("ECH configured but NOT accepted - possible downgrade attack!", .{});
                return error.EchNotAccepted;
            }
            std.log.info("ECH accepted by server", .{});
        }

        return self;
    }

    pub fn read(self: *Tunnel, buffer: []u8) !usize {
        return ssl.read(self.ssl_conn, buffer);
    }

    pub fn write(self: *Tunnel, data: []const u8) !usize {
        return ssl.write(self.ssl_conn, data);
    }

    pub fn getFd(self: *Tunnel) std.posix.socket_t {
        return self.socket;
    }

    pub fn close(self: *Tunnel) void {
        ssl.shutdown(self.ssl_conn);
    }

    pub fn destroy(self: *Tunnel) void {
        ssl.destroySsl(self.ssl_conn);
        ssl.destroyContext(self.ssl_ctx);
        std.posix.close(self.socket);
        self.allocator.free(self.host_z);
        self.allocator.destroy(self);
    }

    pub fn getInfo(self: *Tunnel) !TunnelInfo {
        var info: TunnelInfo = undefined;

        // 获取协议版本
        const version_str = ssl.getVersion(self.ssl_conn);
        info.protocol_version = if (std.mem.eql(u8, version_str, "TLSv1.3"))
            0x0304
        else
            0x0303;

        // 获取 cipher suite
        info.cipher_suite = try ssl.getCipherSuite(self.ssl_conn);

        // ECH 状态（从 BoringSSL 获取）
        info.used_ech = ech.wasAccepted(self.ssl_conn);

        // 服务器名称
        @memset(&info.server_name, 0);
        const name_len = @min(self.config.host.len, 255);
        @memcpy(info.server_name[0..name_len], self.config.host[0..name_len]);

        return info;
    }
};

fn connectTcp(
    allocator: std.mem.Allocator,
    host: []const u8,
    port: u16,
    timeout_ms: u32,
) !std.posix.socket_t {
    _ = timeout_ms; // TODO: 实现超时

    // 解析主机名
    const host_z = try allocator.dupeZ(u8, host);
    defer allocator.free(host_z);

    // 创建地址信息
    const hints = std.posix.addrinfo{
        .flags = std.c.AI.NUMERICSERV,
        .family = std.posix.AF.UNSPEC,
        .socktype = std.posix.SOCK.STREAM,
        .protocol = std.posix.IPPROTO.TCP,
        .canonname = null,
        .addr = null,
        .addrlen = 0,
        .next = null,
    };

    // 端口字符串
    var port_buf: [6]u8 = undefined;
    const port_str = try std.fmt.bufPrintZ(&port_buf, "{d}", .{port});

    // 获取地址信息
    var result: ?*std.posix.addrinfo = null;
    _ = std.c.getaddrinfo(host_z.ptr, port_str.ptr, &hints, &result);
    const addr_list = result orelse return error.ConnectionFailed;
    defer std.c.freeaddrinfo(addr_list);

    // 尝试连接
    var current: ?*std.posix.addrinfo = addr_list;
    while (current) |addr_info| : (current = addr_info.next) {
        const sock = std.posix.socket(
            @intCast(addr_info.family),
            @intCast(addr_info.socktype),
            @intCast(addr_info.protocol),
        ) catch continue;

        std.posix.connect(
            sock,
            addr_info.addr.?,
            @intCast(addr_info.addrlen),
        ) catch {
            std.posix.close(sock);
            continue;
        };

        return sock;
    }

    return error.ConnectionFailed;
}

fn performHandshake(ssl_conn: *ssl.SSL, timeout_ms: u32) !void {
    _ = timeout_ms; // TODO: 实现超时

    try ssl.connect(ssl_conn);
}
