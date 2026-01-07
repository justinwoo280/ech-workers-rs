const std = @import("std");
const tls = @import("zig-tls-tunnel");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    // 解析命令行参数
    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    if (args.len < 3) {
        std.debug.print("Usage: {s} <host> <port>\n", .{args[0]});
        std.debug.print("Example: {s} example.com 443\n", .{args[0]});
        return;
    }

    const host = args[1];
    const port = try std.fmt.parseInt(u16, args[2], 10);

    std.debug.print("Connecting to {s}:{d}...\n", .{ host, port });

    // 配置
    const config = tls.tunnel.TunnelConfig{
        .host = host,
        .port = port,
        .auto_ech = false, // Disable auto ECH (requires dig command)
        .profile = .Firefox120, // Use Firefox fingerprint
    };

    // 创建隧道
    const tunnel = try tls.tunnel.Tunnel.create(allocator, config);
    defer tunnel.destroy();

    std.debug.print("✅ TLS connection established!\n", .{});

    // 获取连接信息
    const info = try tunnel.getInfo();
    
    // 正确显示 TLS 版本 (0x0303 = TLS 1.2, 0x0304 = TLS 1.3)
    const tls_version = switch (info.protocol_version) {
        0x0301 => "TLS 1.0",
        0x0302 => "TLS 1.1",
        0x0303 => "TLS 1.2",
        0x0304 => "TLS 1.3",
        else => "Unknown",
    };
    std.debug.print("Protocol: {s} (0x{X:0>4})\n", .{ tls_version, info.protocol_version });
    
    // 显示 cipher suite 名称
    const cipher_name = switch (info.cipher_suite) {
        0x1301 => "TLS_AES_128_GCM_SHA256",
        0x1302 => "TLS_AES_256_GCM_SHA384",
        0x1303 => "TLS_CHACHA20_POLY1305_SHA256",
        else => "Unknown",
    };
    std.debug.print("Cipher: {s} (0x{X:0>4})\n", .{ cipher_name, info.cipher_suite });
    std.debug.print("ECH: {}\n", .{info.used_ech});

    // 发送 HTTP 请求
    var request_buf: [1024]u8 = undefined;
    const request = try std.fmt.bufPrint(&request_buf, "GET / HTTP/1.1\r\nHost: {s}\r\nConnection: close\r\n\r\n", .{host});
    const n_written = try tunnel.write(request);
    std.debug.print("Sent {d} bytes\n", .{n_written});

    // 读取响应
    var buffer: [4096]u8 = undefined;
    const n_read = try tunnel.read(&buffer);
    std.debug.print("Received {d} bytes:\n", .{n_read});
    std.debug.print("{s}\n", .{buffer[0..n_read]});

    std.debug.print("✅ Test completed successfully!\n", .{});
}
