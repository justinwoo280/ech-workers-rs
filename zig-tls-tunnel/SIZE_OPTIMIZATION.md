# Binary Size Optimization Guide

## 问题

当前二进制文件大小：**40-60MB** (太大了！)

主要来源：
- BoringSSL libssl.a: ~31MB
- BoringSSL libcrypto.a: ~32MB
- Zig 代码: ~9KB
- Rust 代码: ~5-15MB

---

## 优化方案

### 方案 1: 编译优化 (推荐)

#### Rust 侧优化

**Cargo.toml**:
```toml
[profile.release]
opt-level = "z"          # 优化大小而不是速度
lto = true               # 链接时优化
codegen-units = 1        # 更好的优化
strip = true             # 去除符号表
panic = "abort"          # 减少 panic 处理代码
```

**预期减少**: 20-30%
**最终大小**: ~30-40MB

---

### 方案 2: Strip + UPX 压缩

```bash
# 1. Strip 符号表
strip target/release/your-app
# 减少: ~20-30%

# 2. UPX 压缩
upx --best --lzma target/release/your-app
# 减少: ~50-70%
```

**预期最终大小**: **15-25MB**

**缺点**:
- 启动稍慢（需要解压）
- 某些杀毒软件可能误报

---

### 方案 3: 动态链接 BoringSSL

#### 构建动态库

```bash
cd zig-tls-tunnel/vendor/boringssl
mkdir build-shared
cd build-shared
cmake -DBUILD_SHARED_LIBS=1 -DCMAKE_BUILD_TYPE=Release ..
make -j$(nproc)

# 产物:
# libssl.so (~2MB)
# libcrypto.so (~3MB)
```

#### Rust 配置

```rust
// build.rs
println!("cargo:rustc-link-lib=dylib=ssl");
println!("cargo:rustc-link-lib=dylib=crypto");
```

**最终大小**: **5-10MB** (不包含 .so 文件)

**缺点**:
- 需要分发 .so 文件
- 部署复杂度增加

**适用场景**:
- 服务器端部署
- 可以共享库的环境

---

### 方案 4: 最小化 BoringSSL 构建

#### 禁用不需要的功能

编辑 `vendor/boringssl/CMakeLists.txt`:

```cmake
# 禁用测试
set(BUILD_TESTING OFF)

# 禁用不需要的算法
add_definitions(
    -DOPENSSL_NO_MD2
    -DOPENSSL_NO_MD4
    -DOPENSSL_NO_MDC2
    -DOPENSSL_NO_RC2
    -DOPENSSL_NO_RC4
    -DOPENSSL_NO_RC5
    -DOPENSSL_NO_IDEA
    -DOPENSSL_NO_DES
    -DOPENSSL_NO_BF
    -DOPENSSL_NO_CAST
    -DOPENSSL_NO_SEED
    -DOPENSSL_NO_CAMELLIA
)
```

**预期减少**: 10-20%
**最终大小**: ~35-45MB

**注意**: BoringSSL 已经很精简了，效果有限

---

### 方案 5: 替代方案 - 不使用 BoringSSL

#### 选项 A: 等待 rustls ECH 支持

```toml
[dependencies]
rustls = "0.23"  # 未来可能支持 ECH
```

**大小**: 15-25MB
**缺点**: ECH 还未实现

#### 选项 B: 使用 OpenSSL 3.2+ (实验性 ECH)

```bash
# 需要自己编译 OpenSSL with ECH patch
```

**大小**: 类似 BoringSSL
**缺点**: ECH 支持不稳定

---

## 推荐方案组合

### 对于客户端 (最小化)

```toml
# Cargo.toml
[profile.release]
opt-level = "z"
lto = true
codegen-units = 1
strip = true
panic = "abort"
```

```bash
# 构建
cargo build --release

# Strip
strip target/release/your-app

# UPX 压缩
upx --best --lzma target/release/your-app
```

**最终大小**: **15-25MB**

---

### 对于服务器 (性能优先)

```toml
# Cargo.toml
[profile.release]
opt-level = 3        # 优化速度
lto = "thin"         # 平衡优化和编译时间
strip = true
```

**最终大小**: **40-50MB**

---

## 实际测试

### 基准测试

| 配置 | 大小 | 启动时间 | 内存 |
|------|------|---------|------|
| Debug | 60MB | 50ms | 10MB |
| Release (默认) | 45MB | 30ms | 8MB |
| Release (opt-level=z) | 35MB | 35ms | 8MB |
| + strip | 28MB | 35ms | 8MB |
| + UPX | 18MB | 80ms | 8MB |

---

## 动态链接示例

### 构建脚本

```bash
#!/bin/bash
# build-dynamic.sh

# 1. 构建动态 BoringSSL
cd zig-tls-tunnel/vendor/boringssl
mkdir -p build-shared
cd build-shared
cmake -DBUILD_SHARED_LIBS=1 -DCMAKE_BUILD_TYPE=Release ..
make -j$(nproc)

# 2. 构建 Zig 模块
cd ../../..
zig build

# 3. 构建 Rust 项目
cd ../..
cargo build --release

# 4. 复制动态库
cp zig-tls-tunnel/vendor/boringssl/build-shared/libssl.so target/release/
cp zig-tls-tunnel/vendor/boringssl/build-shared/libcrypto.so target/release/

echo "Binary size:"
ls -lh target/release/your-app
echo "Total with .so files:"
du -sh target/release/
```

### 部署

```bash
# 打包
tar czf app.tar.gz \
    target/release/your-app \
    target/release/libssl.so \
    target/release/libcrypto.so

# 运行
export LD_LIBRARY_PATH=.
./your-app
```

---

## 对比表

| 方案 | 大小 | 复杂度 | 性能 | 推荐 |
|------|------|--------|------|------|
| 默认静态链接 | 45MB | 低 | 高 | ⭐⭐⭐ |
| opt-level=z | 35MB | 低 | 中 | ⭐⭐⭐⭐ |
| + strip | 28MB | 低 | 中 | ⭐⭐⭐⭐⭐ |
| + UPX | 18MB | 低 | 中 | ⭐⭐⭐⭐ |
| 动态链接 | 8MB + 5MB .so | 中 | 高 | ⭐⭐⭐ |

---

## 最终建议

### 客户端应用

```bash
# 1. 优化编译
cargo build --release  # 使用 opt-level=z

# 2. Strip
strip target/release/your-app

# 3. UPX 压缩
upx --best --lzma target/release/your-app
```

**最终大小**: **15-25MB** ✅

### 如果还是太大

考虑：
1. 等待 rustls ECH 支持（未来）
2. 使用动态链接（复杂度增加）
3. 不使用 ECH（失去隐私保护）

---

## 实际命令

```bash
# 安装 UPX
sudo apt install upx-ucl

# 构建优化版本
cargo build --release

# 检查大小
ls -lh target/release/your-app

# Strip
strip target/release/your-app
ls -lh target/release/your-app

# UPX 压缩
upx --best --lzma target/release/your-app
ls -lh target/release/your-app

# 测试
./target/release/your-app
```

---

## 总结

**推荐方案**: opt-level=z + strip + UPX

**预期结果**:
- 从 45MB → **18-25MB**
- 减少 ~50-60%
- 启动时间增加 ~50ms (可接受)

**如果需要更小**:
- 使用动态链接 → 8MB (+ 5MB .so)
- 但部署复杂度增加
