const std = @import("std");
const ssl = @import("ssl.zig");
const ech = @import("ech.zig");
const profiles = @import("profiles.zig");
const ws2_32 = std.os.windows.ws2_32;

// Windows socket 类型
pub const socket_t = ws2_32.SOCKET;
const INVALID_SOCKET = ws2_32.INVALID_SOCKET;

pub const TunnelConfig = struct {
    host: []const u8,
    port: u16,
    ech_config: ?[]const u8 = null,
    
    // ECH enforcement: fail if ECH config provided but not accepted
    // This prevents downgrade attacks where DPI strips ECH
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
    socket: socket_t,
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
        errdefer _ = ws2_32.closesocket(self.socket);

        // 创建 SSL 对象
        self.ssl_conn = try ssl.createSsl(self.ssl_ctx);
        errdefer ssl.destroySsl(self.ssl_conn);

        // 绑定 socket
        try ssl.setFd(self.ssl_conn, self.socket);

        // 设置 SNI
        try ssl.setHostname(self.ssl_conn, self.host_z.ptr);
        
        // Apply fingerprint profile (BoringSSL default with GREASE)
        if (config.profile) |prof| {
            prof.apply(self.ssl_ctx);
        }

        // Configure ECH if provided
        var ech_configured = false;
        if (config.ech_config) |ech_cfg| {
            try ech.configure(self.ssl_conn, ech_cfg);
            ech_configured = true;
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

    pub fn getFd(self: *Tunnel) socket_t {
        return self.socket;
    }

    pub fn close(self: *Tunnel) void {
        ssl.shutdown(self.ssl_conn);
    }

    pub fn destroy(self: *Tunnel) void {
        ssl.destroySsl(self.ssl_conn);
        ssl.destroyContext(self.ssl_ctx);
        _ = ws2_32.closesocket(self.socket);
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
) !socket_t {
    // 初始化 Winsock
    var wsa_data: ws2_32.WSADATA = undefined;
    if (ws2_32.WSAStartup(0x0202, &wsa_data) != 0) {
        return error.ConnectionFailed;
    }

    // 解析主机名
    const host_z = try allocator.dupeZ(u8, host);
    defer allocator.free(host_z);

    // 创建地址信息 (使用 Zig 标准库的字段名)
    var hints: ws2_32.addrinfoa = std.mem.zeroes(ws2_32.addrinfoa);
    hints.family = ws2_32.AF.UNSPEC;
    hints.socktype = ws2_32.SOCK.STREAM;
    hints.protocol = ws2_32.IPPROTO.TCP;

    // 端口字符串
    var port_buf: [6]u8 = undefined;
    const port_str = try std.fmt.bufPrintZ(&port_buf, "{d}", .{port});

    // 获取地址信息
    var result: ?*ws2_32.addrinfoa = null;
    if (ws2_32.getaddrinfo(host_z.ptr, port_str.ptr, &hints, &result) != 0) {
        return error.ConnectionFailed;
    }
    const addr_list = result orelse return error.ConnectionFailed;
    defer ws2_32.freeaddrinfo(addr_list);

    // 尝试连接每个地址
    var current: ?*ws2_32.addrinfoa = addr_list;
    while (current) |addr_info| : (current = addr_info.next) {
        const sock = ws2_32.socket(
            addr_info.family,
            addr_info.socktype,
            @intCast(addr_info.protocol),
        );
        if (sock == INVALID_SOCKET) continue;
        errdefer _ = ws2_32.closesocket(sock);

        // 尝试带超时连接
        if (connectWithTimeout(sock, addr_info.addr.?, @intCast(addr_info.addrlen), timeout_ms)) {
            return sock;
        } else |_| {
            _ = ws2_32.closesocket(sock);
            continue;
        }
    }

    return error.ConnectionFailed;
}

/// 带超时的连接
/// 使用 SO_SNDTIMEO 设置连接超时
fn connectWithTimeout(
    sock: socket_t,
    addr: *const ws2_32.sockaddr,
    addrlen: c_int,
    timeout_ms: u32,
) !void {
    // Windows 使用 DWORD 毫秒作为超时值
    const timeout: u32 = timeout_ms;
    
    // SO_SNDTIMEO 用于 connect 超时
    _ = ws2_32.setsockopt(
        sock,
        ws2_32.SOL.SOCKET,
        ws2_32.SO.SNDTIMEO,
        @ptrCast(&timeout),
        @sizeOf(u32),
    );
    
    // 执行连接
    if (ws2_32.connect(sock, addr, addrlen) != 0) {
        const err = ws2_32.WSAGetLastError();
        if (err == ws2_32.WinsockError.WSAETIMEDOUT) {
            std.log.warn("TCP connect timeout after {}ms", .{timeout_ms});
            return error.ConnectionTimedOut;
        }
        return error.ConnectionFailed;
    }
}

fn performHandshake(ssl_conn: *ssl.SSL, timeout_ms: u32) !void {
    _ = timeout_ms; // TODO: 实现超时

    try ssl.connect(ssl_conn);
}
