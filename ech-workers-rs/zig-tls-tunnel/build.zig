const std = @import("std");

pub fn build(b: *std.Build) void {
    const target = b.standardTargetOptions(.{});
    const optimize = b.standardOptimizeOption(.{});

    // ========== BoringSSL 构建 ==========
    
    // BoringSSL 需要 CMake 构建，我们先用预编译的或者系统的 OpenSSL
    // 后续可以添加 BoringSSL 的完整构建
    
    // ========== 主库：libzig-tls-tunnel.a (static) ==========
    
    const lib = b.addStaticLibrary(.{
        .name = "zig-tls-tunnel",
        .root_source_file = b.path("src/api.zig"),  // 直接使用 api.zig
        .target = target,
        .optimize = optimize,
    });

    // 链接 C 标准库
    lib.linkLibC();
    lib.linkLibCpp(); // BoringSSL 需要 C++
    
    // 添加头文件路径
    lib.addIncludePath(b.path("vendor/boringssl/include"));
    
    // 链接 BoringSSL
    lib.addObjectFile(b.path("vendor/boringssl/build/libssl.a"));
    lib.addObjectFile(b.path("vendor/boringssl/build/libcrypto.a"));
    
    // 注意：不打包 BoringSSL 到静态库中
    // Rust 侧需要分别链接：
    // - libzig-tls-tunnel.a (只有 Zig 代码，~10KB)
    // - libssl.a (BoringSSL)
    // - libcrypto.a (BoringSSL)
    
    // Zig 0.13: export symbols are handled by 'export' keyword in source

    b.installArtifact(lib);

    // ========== 测试可执行文件 ==========
    
    const exe = b.addExecutable(.{
        .name = "zig-tls-tunnel-test",
        .root_source_file = b.path("examples/simple_client.zig"),
        .target = target,
        .optimize = optimize,
    });
    
    exe.linkLibC();
    exe.linkLibCpp();
    
    // 链接 BoringSSL
    exe.addIncludePath(b.path("vendor/boringssl/include"));
    exe.addObjectFile(b.path("vendor/boringssl/build/libssl.a"));
    exe.addObjectFile(b.path("vendor/boringssl/build/libcrypto.a"));
    
    exe.root_module.addImport("zig-tls-tunnel", &lib.root_module);

    b.installArtifact(exe);

    // ========== ECH 测试可执行文件 ==========
    
    const ech_exe = b.addExecutable(.{
        .name = "test-ech",
        .root_source_file = b.path("examples/test_ech.zig"),
        .target = target,
        .optimize = optimize,
    });
    
    ech_exe.linkLibC();
    ech_exe.linkLibCpp();
    ech_exe.addIncludePath(b.path("vendor/boringssl/include"));
    ech_exe.addObjectFile(b.path("vendor/boringssl/build/libssl.a"));
    ech_exe.addObjectFile(b.path("vendor/boringssl/build/libcrypto.a"));
    ech_exe.root_module.addImport("zig-tls-tunnel", &lib.root_module);
    
    b.installArtifact(ech_exe);

    // ========== Profile 测试可执行文件 ==========
    
    const profile_exe = b.addExecutable(.{
        .name = "test-profiles",
        .root_source_file = b.path("examples/test_profiles.zig"),
        .target = target,
        .optimize = optimize,
    });
    
    profile_exe.linkLibC();
    profile_exe.linkLibCpp();
    profile_exe.addIncludePath(b.path("vendor/boringssl/include"));
    profile_exe.addObjectFile(b.path("vendor/boringssl/build/libssl.a"));
    profile_exe.addObjectFile(b.path("vendor/boringssl/build/libcrypto.a"));
    profile_exe.root_module.addImport("zig-tls-tunnel", &lib.root_module);
    
    b.installArtifact(profile_exe);

    // ========== 运行命令 ==========
    
    const run_cmd = b.addRunArtifact(exe);
    run_cmd.step.dependOn(b.getInstallStep());

    if (b.args) |args| {
        run_cmd.addArgs(args);
    }

    const run_step = b.step("run", "Run the test client");
    run_step.dependOn(&run_cmd.step);

    // ========== 单元测试 ==========
    
    const lib_unit_tests = b.addTest(.{
        .root_source_file = b.path("src/main.zig"),
        .target = target,
        .optimize = optimize,
    });
    
    lib_unit_tests.linkLibC();
    lib_unit_tests.linkLibCpp();
    lib_unit_tests.addIncludePath(b.path("vendor/boringssl/include"));
    lib_unit_tests.addObjectFile(b.path("vendor/boringssl/build/libssl.a"));
    lib_unit_tests.addObjectFile(b.path("vendor/boringssl/build/libcrypto.a"));

    const run_lib_unit_tests = b.addRunArtifact(lib_unit_tests);

    const test_step = b.step("test", "Run unit tests");
    test_step.dependOn(&run_lib_unit_tests.step);
}
