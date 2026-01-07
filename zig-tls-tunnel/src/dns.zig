/// DNS HTTPS RR query for ECH configuration

const std = @import("std");

// DNS record types
pub const HTTPS_RR_TYPE = 65; // HTTPS RR (RFC 9460)

// HTTPS RR SvcParam keys
pub const SVCPARAM_ECH = 5; // ECH configuration

/// DNS HTTPS RR query result
pub const HttpsRecord = struct {
    priority: u16,
    target: []const u8,
    ech_config: ?[]const u8,
    
    pub fn deinit(self: *HttpsRecord, allocator: std.mem.Allocator) void {
        allocator.free(self.target);
        if (self.ech_config) |config| {
            allocator.free(config);
        }
    }
};

/// Query DNS HTTPS RR record
///
/// Note: This is a simplified implementation using external `dig` command
/// Production should use a proper DNS library (like c-ares)
pub fn queryHttpsRecord(allocator: std.mem.Allocator, domain: []const u8) !?HttpsRecord {
    // Try to use dig command to query HTTPS RR
    const result = std.process.Child.run(.{
        .allocator = allocator,
        .argv = &[_][]const u8{
            "dig",
            "+short",
            "HTTPS",
            domain,
        },
    }) catch |err| {
        std.log.warn("Failed to run dig command: {}", .{err});
        return null;
    };
    defer allocator.free(result.stdout);
    defer allocator.free(result.stderr);
    
    if (result.term.Exited != 0) {
        std.log.warn("dig command failed with exit code {}", .{result.term.Exited});
        return null;
    }
    
    if (result.stdout.len == 0) {
        std.log.info("No HTTPS RR found for {s}", .{domain});
        return null;
    }
    
    // Parse dig output
    // Example format: "1 . ech=..."
    return try parseDigOutput(allocator, result.stdout);
}

fn parseDigOutput(allocator: std.mem.Allocator, output: []const u8) !?HttpsRecord {
    var lines = std.mem.splitScalar(u8, output, '\n');
    
    while (lines.next()) |line| {
        if (line.len == 0) continue;
        
        // Parse first line (highest priority record)
        var parts = std.mem.splitScalar(u8, line, ' ');
        
        // Read priority
        const priority_str = parts.next() orelse continue;
        const priority = std.fmt.parseInt(u16, priority_str, 10) catch continue;
        
        // Read target
        const target_str = parts.next() orelse continue;
        const target = try allocator.dupe(u8, target_str);
        errdefer allocator.free(target);
        
        // Find ech= parameter
        var ech_config: ?[]const u8 = null;
        while (parts.next()) |part| {
            if (std.mem.startsWith(u8, part, "ech=")) {
                const ech_base64 = part[4..];
                // Decode base64
                ech_config = try decodeBase64(allocator, ech_base64);
                break;
            }
        }
        
        return HttpsRecord{
            .priority = priority,
            .target = target,
            .ech_config = ech_config,
        };
    }
    
    return null;
}

fn decodeBase64(allocator: std.mem.Allocator, encoded: []const u8) ![]const u8 {
    const decoder = std.base64.standard.Decoder;
    const max_size = decoder.calcSizeForSlice(encoded) catch return error.InvalidBase64;
    const buffer = try allocator.alloc(u8, max_size);
    errdefer allocator.free(buffer);
    
    decoder.decode(buffer, encoded) catch return error.InvalidBase64;
    return buffer;
}

/// Query using DoH (DNS over HTTPS)
///
/// This is a more modern approach that doesn't depend on system DNS tools
/// TODO: Implement DoH query
pub fn queryHttpsRecordDoH(allocator: std.mem.Allocator, domain: []const u8) !?HttpsRecord {
    _ = allocator;
    _ = domain;
    // TODO: Implement DoH query
    // Can use Cloudflare DoH: https://cloudflare-dns.com/dns-query
    // Or Google DoH: https://dns.google/resolve
    return null;
}
