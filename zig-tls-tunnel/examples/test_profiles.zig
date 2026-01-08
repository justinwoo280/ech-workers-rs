const std = @import("std");
const tls = @import("zig-tls-tunnel");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    if (args.len < 3) {
        std.debug.print("Usage: {s} <host> <port>\n", .{args[0]});
        std.debug.print("Profile: Firefox 120 (only supported profile)\n", .{});
        std.debug.print("Example: {s} example.com 443\n", .{args[0]});
        return;
    }

    const host = args[1];
    const port = try std.fmt.parseInt(u16, args[2], 10);

    // Only Firefox profile is supported
    const profile = tls.profiles.Profile.Firefox120;

    std.debug.print("Testing Firefox fingerprint with {s}:{d}...\n", .{ host, port });

    // Configure tunnel with profile
    const config = tls.tunnel.TunnelConfig{
        .host = host,
        .port = port,
        .profile = profile,
        .auto_ech = false, // Disable auto ECH for testing
    };

    // Create tunnel
    const tunnel = try tls.tunnel.Tunnel.create(allocator, config);
    defer tunnel.destroy();

    std.debug.print("✅ TLS connection established with Firefox fingerprint!\n", .{});

    // Get connection info
    const info = try tunnel.getInfo();
    
    const tls_version = switch (info.protocol_version) {
        0x0301 => "TLS 1.0",
        0x0302 => "TLS 1.1",
        0x0303 => "TLS 1.2",
        0x0304 => "TLS 1.3",
        else => "Unknown",
    };
    std.debug.print("Protocol: {s} (0x{X:0>4})\n", .{ tls_version, info.protocol_version });
    
    const cipher_name = switch (info.cipher_suite) {
        0x1301 => "TLS_AES_128_GCM_SHA256",
        0x1302 => "TLS_AES_256_GCM_SHA384",
        0x1303 => "TLS_CHACHA20_POLY1305_SHA256",
        else => "Unknown",
    };
    std.debug.print("Cipher: {s} (0x{X:0>4})\n", .{ cipher_name, info.cipher_suite });
    std.debug.print("ECH: {}\n", .{info.used_ech});

    // Send HTTP request
    var request_buf: [1024]u8 = undefined;
    const request = try std.fmt.bufPrint(&request_buf, "GET / HTTP/1.1\r\nHost: {s}\r\nConnection: close\r\n\r\n", .{host});
    const n_written = try tunnel.write(request);
    std.debug.print("Sent {d} bytes\n", .{n_written});

    // Read response
    var buffer: [4096]u8 = undefined;
    const n_read = try tunnel.read(&buffer);
    std.debug.print("Received {d} bytes\n", .{n_read});

    // Print first line
    var lines = std.mem.splitScalar(u8, buffer[0..n_read], '\n');
    if (lines.next()) |first_line| {
        std.debug.print("Response: {s}\n", .{first_line});
    }

    std.debug.print("✅ Test completed with Firefox fingerprint!\n", .{});
}
