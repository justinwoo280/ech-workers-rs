# Browser Fingerprint: Firefox 120

## Overview

This module mimics Firefox 120 TLS fingerprint to evade detection.

Based on [utls library](https://github.com/refraction-networking/utls) HelloFirefox_120.

## Why Only Firefox?

### Chrome Problem: ECH GREASE

Chrome sends **ECH GREASE** when no real ECH config is available:
- Outer SNI: **real domain** (exposed to DPI)
- ECH extension: fake/GREASE
- DPI can see: your target domain + your intent to use ECH
- Result: **Easy to block**

### Firefox Advantage: No GREASE

Firefox **never** sends ECH GREASE:
- No ECH config → No ECH extension at all
- Real ECH config → Real ECH with outer SNI = cloudflare-ech.com
- DPI cannot detect "intent to use ECH"
- Result: **Safer**

## Firefox 120 Characteristics

| Feature | Value |
|---------|-------|
| TLS Version | 1.3 |
| Supported Groups | X25519, P-256, P-384, P-521 |
| ALPN | h2, http/1.1 |
| ECH GREASE | Never used |
| Cipher Suites | BoringSSL default order |

### Cipher Suite Order

BoringSSL default (cannot be changed):
1. TLS_AES_128_GCM_SHA256
2. TLS_AES_256_GCM_SHA384
3. TLS_CHACHA20_POLY1305_SHA256

Note: Real Firefox uses different order (CHACHA20 before AES_256), but we cannot change this in BoringSSL.

## Usage

### Zig API

```zig
const tls = @import("zig-tls-tunnel");

const config = tls.tunnel.TunnelConfig{
    .host = "example.com",
    .port = 443,
    .profile = .Firefox120,
};

const tunnel = try tls.tunnel.Tunnel.create(allocator, config);
defer tunnel.destroy();
```

### With ECH

```zig
const config = tls.tunnel.TunnelConfig{
    .host = "example.com",
    .port = 443,
    .profile = .Firefox120,
    .ech_config = ech_config_bytes,  // From DNS HTTPS RR
    .enforce_ech = true,
};

const tunnel = try tls.tunnel.Tunnel.create(allocator, config);
```

### Test Tool

```bash
# Test Firefox fingerprint
./zig-out/bin/test-profiles example.com 443
```

## Compatibility with ECH

Firefox fingerprint is **fully compatible** with ECH:

| Scenario | ECH Extension | Outer SNI | Safe? |
|----------|--------------|-----------|-------|
| No ECH config | None | example.com | ⚠️ Medium |
| Real ECH config | Real ECH | cloudflare-ech.com | ✅ High |
| ~~GREASE ECH~~ | ~~Fake~~ | ~~example.com~~ | ~~❌ Low~~ |

Firefox never uses GREASE ECH, so we only have 2 modes:
1. **Real ECH** (safe)
2. **No ECH** (medium safety)

## Implementation Details

### BoringSSL APIs Used

```c
// Supported Groups
SSL_set1_groups_list(ssl, "X25519:P-256:P-384:P-521");

// ALPN
SSL_set_alpn_protos(ssl, "\x02h2\x08http/1.1", 12);

// ECH (only when config provided)
SSL_set1_ech_config_list(ssl, config, len);
```

### What We Control

✅ Supported groups order
✅ ALPN protocols
✅ ECH configuration

### What We Cannot Control

❌ TLS 1.3 cipher suite order (BoringSSL hardcoded)
❌ Extension order (BoringSSL controlled)
❌ Signature algorithms (BoringSSL defaults)

## Limitations

### Minor Differences from Real Firefox

1. **Cipher Order**: BoringSSL uses different order than Firefox
   - Firefox: AES_128, CHACHA20, AES_256
   - BoringSSL: AES_128, AES_256, CHACHA20

2. **Extensions**: BoringSSL controls extension order internally

These differences are **minor** and unlikely to be detected in practice.

## Security

### JA3 Fingerprint

JA3 hash includes:
- TLS Version ✅ (1.3)
- Cipher Suites ⚠️ (order differs)
- Extensions ⚠️ (order controlled by BoringSSL)
- Supported Groups ✅ (correct)
- Signature Algorithms ⚠️ (BoringSSL defaults)

**Result**: Close to Firefox, but not perfect. Good enough for most scenarios.

### JA4 Fingerprint

JA4 is more lenient:
- TLS Version ✅
- SNI presence ✅
- Cipher count ✅
- Extension count ✅
- ALPN first value ✅

**Result**: Very close to Firefox.

## Testing

### Verify Fingerprint

```bash
# Connect to fingerprint testing service
./test-profiles tls.peet.ws 443

# Compare with real Firefox
curl https://tls.peet.ws/api/all
```

### Test with ECH

```bash
# With ECH config
./test-ech cloudflare.com 443 <base64_ech_config>
```

## References

- [utls HelloFirefox_120](https://github.com/refraction-networking/utls/blob/master/u_parrots.go)
- [Firefox TLS Configuration](https://wiki.mozilla.org/Security/Server_Side_TLS)
- [JA3 Specification](https://github.com/salesforce/ja3)
- [JA4 Specification](https://github.com/FoxIO-LLC/ja4)

## Summary

**Firefox 120 profile is the only supported profile** because:
1. ✅ Never uses ECH GREASE (safe)
2. ✅ Compatible with real ECH
3. ✅ Good fingerprint match
4. ✅ Simple and reliable

Chrome was removed because it uses ECH GREASE, which exposes intent without protection.
