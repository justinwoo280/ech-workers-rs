/// TLS Fingerprint Profiles
///
/// Strategy: Mimic real browser fingerprints for better anonymity
///
/// Note: We do NOT use ECH GREASE - either use real ECH or nothing

const std = @import("std");
const ssl = @import("ssl.zig");

pub const Profile = enum {
    /// BoringSSL default with GREASE enabled
    /// Minimal fingerprint, blends with gRPC, Cloudflare, etc.
    BoringSSLDefault,
    
    /// Chrome 120+ fingerprint (recommended)
    /// Full Chrome-like fingerprint with all extensions
    Chrome,
    
    /// Firefox 120 fingerprint (placeholder for future WolfSSL/NSS implementation)
    /// Currently falls back to BoringSSLDefault
    Firefox120,
    
    /// Apply profile to SSL context (called once per context)
    pub fn applyCtx(self: Profile, ctx: *ssl.SSL_CTX) void {
        switch (self) {
            .BoringSSLDefault => applyBoringSSLDefault(ctx),
            .Chrome => applyChromeCtx(ctx),
            .Firefox120 => applyBoringSSLDefault(ctx), // TODO: WolfSSL/NSS
        }
    }
    
    /// Apply profile to SSL connection (called per connection)
    pub fn applySsl(self: Profile, ssl_conn: *ssl.SSL) void {
        switch (self) {
            .BoringSSLDefault => {},
            .Chrome => applyChromeSsl(ssl_conn),
            .Firefox120 => {},
        }
    }
    
    // Keep old API for compatibility
    pub fn apply(self: Profile, ctx: *ssl.SSL_CTX) void {
        self.applyCtx(ctx);
    }
};

// ========== Chrome Fingerprint ==========
// Reference: Chrome 120+ ClientHello
// JA3: c180412f163c5e0b48d3f5d64dd4004b

/// Chrome cipher list (TLS 1.3 + TLS 1.2 for fingerprint)
/// We only use TLS 1.3, but declare TLS 1.2 ciphers to look like Chrome
const CHROME_CIPHER_LIST: [*:0]const u8 = 
    "TLS_AES_128_GCM_SHA256:" ++
    "TLS_AES_256_GCM_SHA384:" ++
    "TLS_CHACHA20_POLY1305_SHA256:" ++
    "ECDHE-ECDSA-AES128-GCM-SHA256:" ++
    "ECDHE-RSA-AES128-GCM-SHA256:" ++
    "ECDHE-ECDSA-AES256-GCM-SHA384:" ++
    "ECDHE-RSA-AES256-GCM-SHA384:" ++
    "ECDHE-ECDSA-CHACHA20-POLY1305:" ++
    "ECDHE-RSA-CHACHA20-POLY1305:" ++
    "ECDHE-RSA-AES128-SHA:" ++
    "ECDHE-RSA-AES256-SHA:" ++
    "AES128-GCM-SHA256:" ++
    "AES256-GCM-SHA384:" ++
    "AES128-SHA:" ++
    "AES256-SHA";

/// Chrome ALPN: h2, http/1.1
const CHROME_ALPN = "\x02h2\x08http/1.1";

/// Chrome supported groups (with ML-KEM for Post-Quantum)
/// Order: X25519MLKEM768, X25519, P-256, P-384
const CHROME_GROUPS = [_]u16{
    ssl.SSL_GROUP_X25519_MLKEM768, // Post-Quantum hybrid (makes ClientHello ~1KB larger)
    ssl.SSL_GROUP_X25519,
    ssl.SSL_GROUP_SECP256R1,
    ssl.SSL_GROUP_SECP384R1,
};

/// Apply Chrome fingerprint to context (one-time setup)
fn applyChromeCtx(ctx: *ssl.SSL_CTX) void {
    // Enable GREASE
    ssl.setGreaseEnabled(ctx, true);
    
    // Enable extension permutation
    ssl.setPermuteExtensions(ctx, true);
    
    // Set Chrome cipher list (declares TLS 1.2 ciphers for fingerprint)
    ssl.setCipherListCtx(ctx, CHROME_CIPHER_LIST) catch {};
    
    // Set Chrome groups with ML-KEM (Post-Quantum)
    ssl.setGroupIdsCtx(ctx, &CHROME_GROUPS) catch {};
    
    // Enable OCSP stapling (status_request extension)
    ssl.enableOcspStaplingCtx(ctx);
    
    // Enable Signed Certificate Timestamps
    ssl.enableSignedCertTimestampsCtx(ctx);
    
    // Enable brotli certificate compression
    ssl.enableCertDecompressionBrotli(ctx) catch {};
    
    // Set ALPN: h2, http/1.1
    ssl.setAlpnProtosCtx(ctx, CHROME_ALPN) catch {};
}

/// Apply Chrome fingerprint to SSL connection (per-connection setup)
fn applyChromeSsl(ssl_conn: *ssl.SSL) void {
    // Add ALPS for h2 (Google extension)
    // Chrome sends empty settings for h2
    ssl.addApplicationSettings(ssl_conn, "h2", "") catch {};
}

// ========== BoringSSL Default ==========

/// BoringSSL default fingerprint with GREASE
fn applyBoringSSLDefault(ctx: *ssl.SSL_CTX) void {
    // Enable GREASE (NOT ECH GREASE, just regular GREASE)
    ssl.setGreaseEnabled(ctx, true);
    
    // Enable extension permutation
    ssl.setPermuteExtensions(ctx, true);
    
    // Use BoringSSL defaults for everything else
}
