const std = @import("std");
const tls = @import("zig-tls-tunnel");

pub fn main() !void {
    var gpa = std.heap.GeneralPurposeAllocator(.{}){};
    defer _ = gpa.deinit();
    const allocator = gpa.allocator();

    const args = try std.process.argsAlloc(allocator);
    defer std.process.argsFree(allocator, args);

    if (args.len < 3) {
        std.debug.print("Usage: {s} <host> <port> [ech_config_base64]\n", .{args[0]});
        std.debug.print("Example: {s} cloudflare.com 443\n", .{args[0]});
        std.debug.print("Example with ECH: {s} cloudflare.com 443 <base64_config>\n", .{args[0]});
        return;
    }

    const host = args[1];
    const port = try std.fmt.parseInt(u16, args[2], 10);

    std.debug.print("Testing ECH with {s}:{d}...\n", .{ host, port });

    // Decode ECH config if provided
    var ech_config: ?[]const u8 = null;
    defer if (ech_config) |cfg| allocator.free(cfg);

    if (args.len >= 4) {
        const ech_base64 = args[3];
        std.debug.print("Decoding ECH config from base64...\n", .{});
        
        const decoder = std.base64.standard.Decoder;
        const max_size = decoder.calcSizeForSlice(ech_base64) catch {
            std.debug.print("❌ Invalid base64 ECH config\n", .{});
            return error.InvalidBase64;
        };
        
        const buffer = try allocator.alloc(u8, max_size);
        decoder.decode(buffer, ech_base64) catch {
            allocator.free(buffer);
            std.debug.print("❌ Failed to decode base64 ECH config\n", .{});
            return error.InvalidBase64;
        };
        
        ech_config = buffer;
        std.debug.print("✅ ECH config decoded: {d} bytes\n", .{buffer.len});
    }

    // Configure tunnel
    const config = tls.tunnel.TunnelConfig{
        .host = host,
        .port = port,
        .ech_config = ech_config,
        .profile = .Firefox120,
    };

    // Create tunnel
    const tunnel = try tls.tunnel.Tunnel.create(allocator, config);
    defer tunnel.destroy();

    std.debug.print("✅ TLS connection established!\n", .{});

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
    
    if (info.used_ech) {
        std.debug.print("ECH: ✅ ACCEPTED\n", .{});
    } else {
        std.debug.print("ECH: ❌ NOT USED or REJECTED\n", .{});
    }

    // Send HTTP request
    var request_buf: [1024]u8 = undefined;
    const request = try std.fmt.bufPrint(&request_buf, "GET / HTTP/1.1\r\nHost: {s}\r\nConnection: close\r\n\r\n", .{host});
    const n_written = try tunnel.write(request);
    std.debug.print("Sent {d} bytes\n", .{n_written});

    // Read response
    var buffer: [4096]u8 = undefined;
    const n_read = try tunnel.read(&buffer);
    std.debug.print("Received {d} bytes\n", .{n_read});

    // Print first line of response
    var lines = std.mem.splitScalar(u8, buffer[0..n_read], '\n');
    if (lines.next()) |first_line| {
        std.debug.print("Response: {s}\n", .{first_line});
    }

    std.debug.print("✅ Test completed!\n", .{});
}
