/// Browser fingerprint profiles for JA3/JA4 mimicry
///
/// Based on utls library: https://github.com/refraction-networking/utls

const std = @import("std");
const ssl = @import("ssl.zig");

pub const Profile = enum {
    Firefox120,
    
    pub fn apply(self: Profile, ctx: *ssl.SSL_CTX, ssl_conn: *ssl.SSL, has_real_ech: bool) !void {
        switch (self) {
            .Firefox120 => try applyFirefox120(ctx, ssl_conn, has_real_ech),
        }
    }
};

/// Firefox 120 fingerprint
/// Reference: utls HelloFirefox_120
/// 
/// Firefox never uses ECH GREASE, making it perfect for our use case
fn applyFirefox120(ctx: *ssl.SSL_CTX, ssl_conn: *ssl.SSL, has_real_ech: bool) !void {
    _ = ctx;
    _ = has_real_ech;
    
    // Supported Groups (curves)
    // Firefox 120: X25519, P256, P384, P521
    try ssl.setGroupsList(ssl_conn, "X25519:P-256:P-384:P-521");
    
    // ALPN protocols
    // Firefox 120: h2, http/1.1
    const alpn = "\x02h2\x08http/1.1";
    try ssl.setAlpnProtos(ssl_conn, alpn);
    
    // Note: Firefox cipher suite order differs from BoringSSL default
    // Firefox: AES_128_GCM, CHACHA20, AES_256_GCM
    // BoringSSL: AES_128_GCM, AES_256_GCM, CHACHA20
    // But we cannot change TLS 1.3 cipher order in BoringSSL
}
