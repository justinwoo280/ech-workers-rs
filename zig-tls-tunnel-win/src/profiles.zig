/// TLS Fingerprint Profiles
///
/// Strategy: Use BoringSSL default fingerprint with GREASE enabled
/// This blends in with Chrome, Android, gRPC, Cloudflare and many other apps
///
/// Note: We do NOT use ECH GREASE - either use real ECH or nothing

const std = @import("std");
const ssl = @import("ssl.zig");

pub const Profile = enum {
    /// BoringSSL default with GREASE enabled (recommended)
    /// Blends in with Chrome, Android, gRPC, Cloudflare, etc.
    BoringSSLDefault,
    
    pub fn apply(self: Profile, ctx: *ssl.SSL_CTX) void {
        switch (self) {
            .BoringSSLDefault => applyBoringSSLDefault(ctx),
        }
    }
};

/// BoringSSL default fingerprint with GREASE
/// 
/// Features:
/// - GREASE enabled (random values in cipher suites, extensions, etc.)
/// - Extension order permutation (randomize extension order)
/// - Default cipher suites, groups, ALPN
/// 
/// This is the most common fingerprint on the internet
fn applyBoringSSLDefault(ctx: *ssl.SSL_CTX) void {
    // Enable GREASE (NOT ECH GREASE, just regular GREASE)
    // This adds random values to prevent protocol ossification
    ssl.setGreaseEnabled(ctx, true);
    
    // Enable extension permutation to randomize extension order
    ssl.setPermuteExtensions(ctx, true);
    
    // Use BoringSSL defaults for everything else:
    // - Cipher suites: AES_128_GCM, AES_256_GCM, CHACHA20
    // - Groups: X25519, P-256, P-384
    // - ALPN: not set by default (will be set per-connection if needed)
}
