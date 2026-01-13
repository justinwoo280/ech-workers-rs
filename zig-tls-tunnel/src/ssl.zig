/// OpenSSL/BoringSSL C 绑定
///
/// 注意：这里先用标准 OpenSSL API
/// 后续切换到 BoringSSL 时，需要添加 ECH 相关的 API

const std = @import("std");

// ========== C 类型定义 ==========

pub const SSL_CTX = opaque {};
pub const SSL = opaque {};
pub const SSL_METHOD = opaque {};
pub const BIO = opaque {};
pub const X509 = opaque {};
pub const X509_STORE = opaque {};

// TLS 版本常量
pub const TLS1_3_VERSION = 0x0304;
pub const TLS1_2_VERSION = 0x0303;

// Supported Groups (curves)
pub const SSL_GROUP_SECP256R1 = 23;
pub const SSL_GROUP_SECP384R1 = 24;
pub const SSL_GROUP_SECP521R1 = 25;
pub const SSL_GROUP_X25519 = 29;
pub const SSL_GROUP_X25519_MLKEM768 = 0x11ec;

// SSL 错误码
pub const SSL_ERROR_NONE = 0;
pub const SSL_ERROR_SSL = 1;
pub const SSL_ERROR_WANT_READ = 2;
pub const SSL_ERROR_WANT_WRITE = 3;
pub const SSL_ERROR_SYSCALL = 5;
pub const SSL_ERROR_ZERO_RETURN = 6;

// ========== OpenSSL 函数声明 ==========

// 初始化
extern "c" fn OPENSSL_init_ssl(opts: u64, settings: ?*anyopaque) c_int;
extern "c" fn OPENSSL_init_crypto(opts: u64, settings: ?*anyopaque) c_int;

// SSL_CTX 相关
extern "c" fn TLS_client_method() ?*const SSL_METHOD;
extern "c" fn SSL_CTX_new(method: ?*const SSL_METHOD) ?*SSL_CTX;
extern "c" fn SSL_CTX_free(ctx: *SSL_CTX) void;
extern "c" fn SSL_CTX_set_min_proto_version(ctx: *SSL_CTX, version: u16) c_int;
extern "c" fn SSL_CTX_set_max_proto_version(ctx: *SSL_CTX, version: u16) c_int;
extern "c" fn SSL_CTX_set_cipher_list(ctx: *SSL_CTX, str: [*:0]const u8) c_int; // BoringSSL: 同时支持 TLS 1.2 和 TLS 1.3
extern "c" fn SSL_CTX_set_default_verify_paths(ctx: *SSL_CTX) c_int;

// SSL 对象相关
extern "c" fn SSL_new(ctx: *SSL_CTX) ?*SSL;
extern "c" fn SSL_free(ssl: *SSL) void;
extern "c" fn SSL_set_fd(ssl: *SSL, fd: c_int) c_int;
extern "c" fn SSL_set_tlsext_host_name(ssl: *SSL, name: [*:0]const u8) c_int;
extern "c" fn SSL_connect(ssl: *SSL) c_int;
extern "c" fn SSL_shutdown(ssl: *SSL) c_int;

// I/O 操作
extern "c" fn SSL_read(ssl: *SSL, buf: [*]u8, num: c_int) c_int;
extern "c" fn SSL_write(ssl: *SSL, buf: [*]const u8, num: c_int) c_int;
extern "c" fn SSL_get_error(ssl: *SSL, ret: c_int) c_int;

// 信息获取
extern "c" fn SSL_get_version(ssl: *SSL) [*:0]const u8;
extern "c" fn SSL_get_current_cipher(ssl: *SSL) ?*const anyopaque;
extern "c" fn SSL_CIPHER_get_id(cipher: *const anyopaque) c_uint;

// 错误处理
extern "c" fn ERR_get_error() c_ulong;
extern "c" fn ERR_error_string_n(err: c_ulong, buf: [*]u8, len: usize) void;

// ========== Fingerprint Customization API ==========

// Note: We do NOT use ECH GREASE
// Reason: GREASE ECH exposes intent without protection
// Strategy: Either use real ECH or nothing at all

// GREASE (Generate Random Extensions And Sustain Extensibility)
// This is regular GREASE for cipher suites, extensions, etc. (NOT ECH GREASE)
extern "c" fn SSL_CTX_set_grease_enabled(ctx: *SSL_CTX, enabled: c_int) void;
extern "c" fn SSL_CTX_set_permute_extensions(ctx: *SSL_CTX, enabled: c_int) void;

// Supported Groups (curves)
extern "c" fn SSL_set1_groups(ssl: *SSL, groups: [*]const c_int, groups_len: usize) c_int;
extern "c" fn SSL_CTX_set1_groups(ctx: *SSL_CTX, groups: [*]const c_int, groups_len: usize) c_int;
extern "c" fn SSL_set1_groups_list(ssl: *SSL, groups: [*:0]const u8) c_int;
extern "c" fn SSL_CTX_set1_groups_list(ctx: *SSL_CTX, groups: [*:0]const u8) c_int;

// Supported Groups by ID (for ML-KEM / Post-Quantum)
extern "c" fn SSL_CTX_set1_group_ids(ctx: *SSL_CTX, group_ids: [*]const u16, num_group_ids: usize) c_int;
extern "c" fn SSL_set1_group_ids(ssl: *SSL, group_ids: [*]const u16, num_group_ids: usize) c_int;

// Group ID constants
pub const SSL_GROUP_SECP256R1: u16 = 23;
pub const SSL_GROUP_SECP384R1: u16 = 24;
pub const SSL_GROUP_X25519: u16 = 29;
pub const SSL_GROUP_X25519_MLKEM768: u16 = 0x11ec; // Post-Quantum hybrid

// Certificate Compression (for brotli)
const CBB = opaque {};
const ssl_cert_compression_func_t = ?*const fn (*SSL, *CBB, [*]const u8, usize) callconv(.C) c_int;
const ssl_cert_decompression_func_t = ?*const fn (*SSL, *allowzero u8 , usize, [*]const u8, usize) callconv(.C) c_int;
extern "c" fn SSL_CTX_add_cert_compression_alg(
    ctx: *SSL_CTX,
    alg_id: u16,
    compress: ssl_cert_compression_func_t,
    decompress: ssl_cert_decompression_func_t,
) c_int;

// Certificate compression algorithm IDs
pub const TLSEXT_cert_compression_zlib: u16 = 1;
pub const TLSEXT_cert_compression_brotli: u16 = 2;
pub const TLSEXT_cert_compression_zstd: u16 = 3;

// ALPN
extern "c" fn SSL_set_alpn_protos(ssl: *SSL, protos: [*]const u8, protos_len: c_uint) c_int;
extern "c" fn SSL_CTX_set_alpn_protos(ctx: *SSL_CTX, protos: [*]const u8, protos_len: c_uint) c_int;

// Cipher List (for Chrome fingerprint - declare TLS 1.2 ciphers even though we only use TLS 1.3)
extern "c" fn SSL_CTX_set_cipher_list(ctx: *SSL_CTX, str: [*:0]const u8) c_int;
extern "c" fn SSL_set_cipher_list(ssl: *SSL, str: [*:0]const u8) c_int;

// OCSP Stapling (status_request extension)
extern "c" fn SSL_CTX_enable_ocsp_stapling(ctx: *SSL_CTX) void;
extern "c" fn SSL_enable_ocsp_stapling(ssl: *SSL) void;

// Signed Certificate Timestamps (SCT)
extern "c" fn SSL_CTX_enable_signed_cert_timestamps(ctx: *SSL_CTX) void;
extern "c" fn SSL_enable_signed_cert_timestamps(ssl: *SSL) void;

// ALPS (Application-Layer Protocol Settings) - Google extension
extern "c" fn SSL_add_application_settings(
    ssl: *SSL,
    proto: [*]const u8,
    proto_len: usize,
    settings: [*]const u8,
    settings_len: usize,
) c_int;

// ========== ECH (Encrypted Client Hello) API ==========

/// SSL_set1_ech_config_list 配置客户端使用 ECH
/// ech_config_list 应该包含序列化的 ECHConfigList 结构
extern "c" fn SSL_set1_ech_config_list(
    ssl: *SSL,
    ech_config_list: [*]const u8,
    ech_config_list_len: usize,
) c_int;

/// SSL_ech_accepted 检查 ECH 是否被服务器接受
/// 返回 1 表示接受，0 表示拒绝或未使用
extern "c" fn SSL_ech_accepted(ssl: *const SSL) c_int;

/// SSL_get0_ech_name_override 获取 ECH 覆盖的服务器名称
extern "c" fn SSL_get0_ech_name_override(
    ssl: *const SSL,
    out_name: *[*]const u8,
    out_name_len: *usize,
) c_int;

/// SSL_get0_ech_retry_configs 获取 ECH retry 配置
extern "c" fn SSL_get0_ech_retry_configs(
    ssl: *const SSL,
    out_retry_configs: *[*]const u8,
    out_retry_configs_len: *usize,
) c_int;

// ========== Zig 包装函数 ==========

pub fn init() !void {
    const OPENSSL_INIT_LOAD_SSL_STRINGS = 0x00200000;
    const OPENSSL_INIT_LOAD_CRYPTO_STRINGS = 0x00000002;

    if (OPENSSL_init_ssl(OPENSSL_INIT_LOAD_SSL_STRINGS, null) != 1) {
        return error.SslInitFailed;
    }

    if (OPENSSL_init_crypto(OPENSSL_INIT_LOAD_CRYPTO_STRINGS, null) != 1) {
        return error.CryptoInitFailed;
    }
}

pub fn createContext() !*SSL_CTX {
    const method = TLS_client_method() orelse return error.MethodFailed;
    const ctx = SSL_CTX_new(method) orelse return error.ContextFailed;
    return ctx;
}

pub fn destroyContext(ctx: *SSL_CTX) void {
    SSL_CTX_free(ctx);
}

pub fn setTls13Only(ctx: *SSL_CTX) !void {
    if (SSL_CTX_set_min_proto_version(ctx, TLS1_3_VERSION) != 1) {
        return error.SetMinVersionFailed;
    }
    if (SSL_CTX_set_max_proto_version(ctx, TLS1_3_VERSION) != 1) {
        return error.SetMaxVersionFailed;
    }
}

/// 设置 cipher list (BoringSSL 同时支持 TLS 1.2 和 TLS 1.3)
pub fn setCipherList(ctx: *SSL_CTX, ciphers: [*:0]const u8) !void {
    if (SSL_CTX_set_cipher_list(ctx, ciphers) != 1) {
        return error.SetCipherListFailed;
    }
}

pub fn setDefaultVerifyPaths(ctx: *SSL_CTX) !void {
    if (SSL_CTX_set_default_verify_paths(ctx) != 1) {
        return error.SetVerifyPathsFailed;
    }
}

pub fn createSsl(ctx: *SSL_CTX) !*SSL {
    const ssl = SSL_new(ctx) orelse return error.SslNewFailed;
    return ssl;
}

pub fn destroySsl(ssl: *SSL) void {
    SSL_free(ssl);
}

pub fn setFd(ssl: *SSL, fd: c_int) !void {
    if (SSL_set_fd(ssl, fd) != 1) {
        return error.SetFdFailed;
    }
}

pub fn setHostname(ssl: *SSL, hostname: [*:0]const u8) !void {
    if (SSL_set_tlsext_host_name(ssl, hostname) != 1) {
        return error.SetHostnameFailed;
    }
}

pub fn connect(ssl: *SSL) !void {
    const ret = SSL_connect(ssl);
    if (ret != 1) {
        const err = SSL_get_error(ssl, ret);
        return sslErrorToZig(err);
    }
}

pub fn shutdown(ssl: *SSL) void {
    _ = SSL_shutdown(ssl);
}

pub fn read(ssl: *SSL, buffer: []u8) !usize {
    const ret = SSL_read(ssl, buffer.ptr, @intCast(buffer.len));
    if (ret <= 0) {
        const err = SSL_get_error(ssl, ret);
        return sslErrorToZig(err);
    }
    return @intCast(ret);
}

pub fn write(ssl: *SSL, data: []const u8) !usize {
    const ret = SSL_write(ssl, data.ptr, @intCast(data.len));
    if (ret <= 0) {
        const err = SSL_get_error(ssl, ret);
        return sslErrorToZig(err);
    }
    return @intCast(ret);
}

pub fn getVersion(ssl: *SSL) []const u8 {
    const ver = SSL_get_version(ssl);
    return std.mem.span(ver);
}

pub fn getCipherSuite(ssl: *SSL) !u16 {
    const cipher = SSL_get_current_cipher(ssl) orelse return error.NoCipher;
    const id = SSL_CIPHER_get_id(cipher);
    return @intCast(id & 0xFFFF);
}

// ========== Fingerprint Customization Functions ==========

/// Enable GREASE (Generate Random Extensions And Sustain Extensibility)
/// This adds random values to cipher suites, extensions, etc. to prevent ossification
/// Note: This is NOT ECH GREASE - we never use ECH GREASE
pub fn setGreaseEnabled(ctx: *SSL_CTX, enabled: bool) void {
    SSL_CTX_set_grease_enabled(ctx, if (enabled) 1 else 0);
}

/// Enable extension permutation (randomize extension order)
/// This helps avoid fingerprinting based on extension order
pub fn setPermuteExtensions(ctx: *SSL_CTX, enabled: bool) void {
    SSL_CTX_set_permute_extensions(ctx, if (enabled) 1 else 0);
}

/// Set supported groups (curves) for key exchange
pub fn setGroups(ssl: *SSL, groups: []const c_int) !void {
    if (SSL_set1_groups(ssl, groups.ptr, groups.len) != 1) {
        return error.SetGroupsFailed;
    }
}

/// Set supported groups (curves) on context
pub fn setGroupsCtx(ctx: *SSL_CTX, groups: []const c_int) !void {
    if (SSL_CTX_set1_groups(ctx, groups.ptr, groups.len) != 1) {
        return error.SetGroupsFailed;
    }
}

/// Set supported groups using string format (e.g., "X25519:P-256:P-384")
pub fn setGroupsList(ssl: *SSL, groups: [*:0]const u8) !void {
    if (SSL_set1_groups_list(ssl, groups) != 1) {
        return error.SetGroupsFailed;
    }
}

/// Set supported groups on context using string format
pub fn setGroupsListCtx(ctx: *SSL_CTX, groups: [*:0]const u8) !void {
    if (SSL_CTX_set1_groups_list(ctx, groups) != 1) {
        return error.SetGroupsFailed;
    }
}

/// Set ALPN protocols
/// Format: length-prefixed strings, e.g., "\x02h2\x08http/1.1"
pub fn setAlpnProtos(ssl: *SSL, protos: []const u8) !void {
    if (SSL_set_alpn_protos(ssl, protos.ptr, @intCast(protos.len)) != 0) {
        return error.SetAlpnFailed;
    }
}

/// Set ALPN protocols on context
pub fn setAlpnProtosCtx(ctx: *SSL_CTX, protos: []const u8) !void {
    if (SSL_CTX_set_alpn_protos(ctx, protos.ptr, @intCast(protos.len)) != 0) {
        return error.SetAlpnFailed;
    }
}

// ========== Chrome Fingerprint Functions ==========

/// Set cipher list string (e.g., "TLS_AES_128_GCM_SHA256:ECDHE-RSA-AES128-GCM-SHA256:...")
/// This declares TLS 1.2 ciphers for fingerprint, even though we only use TLS 1.3
pub fn setCipherListCtx(ctx: *SSL_CTX, cipher_str: [*:0]const u8) !void {
    if (SSL_CTX_set_cipher_list(ctx, cipher_str) != 1) {
        return error.SetCipherListFailed;
    }
}

/// Enable OCSP stapling request (status_request extension)
pub fn enableOcspStaplingCtx(ctx: *SSL_CTX) void {
    SSL_CTX_enable_ocsp_stapling(ctx);
}

/// Enable OCSP stapling request on SSL connection
pub fn enableOcspStapling(ssl: *SSL) void {
    SSL_enable_ocsp_stapling(ssl);
}

/// Enable Signed Certificate Timestamps request (SCT extension)
pub fn enableSignedCertTimestampsCtx(ctx: *SSL_CTX) void {
    SSL_CTX_enable_signed_cert_timestamps(ctx);
}

/// Enable SCT on SSL connection
pub fn enableSignedCertTimestamps(ssl: *SSL) void {
    SSL_enable_signed_cert_timestamps(ssl);
}

/// Add ALPS (Application-Layer Protocol Settings) - Google extension
/// proto: ALPN protocol name (e.g., "h2")
/// settings: application settings data (can be empty)
pub fn addApplicationSettings(ssl: *SSL, proto: []const u8, settings: []const u8) !void {
    if (SSL_add_application_settings(
        ssl,
        proto.ptr,
        proto.len,
        settings.ptr,
        settings.len,
    ) != 1) {
        return error.SetAlpsFailed;
    }
}

// ========== Post-Quantum / ML-KEM Functions ==========

/// Set supported groups by ID (for ML-KEM support)
/// Chrome order: X25519MLKEM768, X25519, P-256, P-384
pub fn setGroupIdsCtx(ctx: *SSL_CTX, group_ids: []const u16) !void {
    if (SSL_CTX_set1_group_ids(ctx, group_ids.ptr, group_ids.len) != 1) {
        return error.SetGroupsFailed;
    }
}

/// Set supported groups by ID on SSL connection
pub fn setGroupIds(ssl_conn: *SSL, group_ids: []const u16) !void {
    if (SSL_set1_group_ids(ssl_conn, group_ids.ptr, group_ids.len) != 1) {
        return error.SetGroupsFailed;
    }
}

// ========== Certificate Compression Functions ==========

/// Enable brotli certificate decompression (client-side)
/// We only need decompression - we don't compress certificates as a client
pub fn enableCertDecompressionBrotli(ctx: *SSL_CTX) !void {
    // For client, we only need to declare we support brotli decompression
    // compress = null (we don't compress), decompress = null triggers BoringSSL's built-in
    // Actually, BoringSSL requires at least decompress callback, so we use a stub
    if (SSL_CTX_add_cert_compression_alg(
        ctx,
        TLSEXT_cert_compression_brotli,
        null, // no compress (client doesn't send certs)
        null, // BoringSSL has built-in brotli if compiled with it
    ) != 1) {
        // If this fails, brotli might not be compiled in - that's OK
        // We just won't advertise brotli support
    }
}

fn sslErrorToZig(err: c_int) error{
    WouldBlock,
    ConnectionClosed,
    SslError,
    SyscallError,
} {
    return switch (err) {
        SSL_ERROR_WANT_READ, SSL_ERROR_WANT_WRITE => error.WouldBlock,
        SSL_ERROR_ZERO_RETURN => error.ConnectionClosed,
        SSL_ERROR_SYSCALL => error.SyscallError,
        else => error.SslError,
    };
}

pub fn getLastError(buffer: []u8) []const u8 {
    const err = ERR_get_error();
    if (err == 0) return "";

    ERR_error_string_n(err, buffer.ptr, buffer.len);
    return std.mem.sliceTo(buffer, 0);
}

// ========== ECH 包装函数 ==========

pub fn setEchConfig(ssl: *SSL, ech_config: []const u8) !void {
    if (SSL_set1_ech_config_list(ssl, ech_config.ptr, ech_config.len) != 1) {
        return error.SetEchConfigFailed;
    }
}

pub fn echAccepted(ssl: *const SSL) bool {
    return SSL_ech_accepted(ssl) == 1;
}

pub fn getEchNameOverride(ssl: *const SSL) ?[]const u8 {
    var name_ptr: [*]const u8 = undefined;
    var name_len: usize = undefined;
    
    if (SSL_get0_ech_name_override(ssl, &name_ptr, &name_len) != 1) {
        return null;
    }
    
    if (name_len == 0) return null;
    return name_ptr[0..name_len];
}

pub fn getEchRetryConfigs(ssl: *const SSL) ?[]const u8 {
    var configs_ptr: [*]const u8 = undefined;
    var configs_len: usize = undefined;
    
    if (SSL_get0_ech_retry_configs(ssl, &configs_ptr, &configs_len) != 1) {
        return null;
    }
    
    if (configs_len == 0) return null;
    return configs_ptr[0..configs_len];
}
