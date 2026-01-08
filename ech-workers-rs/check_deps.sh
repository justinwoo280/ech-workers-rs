#!/bin/bash
# 检查依赖兼容性

echo "=== Checking jarustls version ==="
grep "^version" ../rustls/Cargo.toml

echo ""
echo "=== Checking tokio-rustls compatibility ==="
echo "tokio-rustls 0.25 expects rustls ~0.23"
echo "tokio-rustls 0.26 expects rustls ~0.24"

echo ""
echo "=== Recommendation ==="
echo "Option 1: Use tokio-rustls 0.26 (if available)"
echo "Option 2: Downgrade jarustls to 0.23.x"
echo "Option 3: Use official rustls for now"

echo ""
echo "=== Testing cargo check (this will fail without Rust installed) ==="
# cargo check 2>&1 | head -20
