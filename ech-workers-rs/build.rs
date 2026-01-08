use std::path::PathBuf;
use std::env;

fn main() {
    // 获取项目根目录 (ech-workers-rs 的父目录)
    let manifest_dir = env::var("CARGO_MANIFEST_DIR").unwrap();
    let project_root = PathBuf::from(&manifest_dir).parent().unwrap().to_path_buf();
    
    let zig_lib_path = project_root.join("zig-tls-tunnel/zig-out/lib");
    let boringssl_path = project_root.join("zig-tls-tunnel/vendor/boringssl/build");
    
    // Link Zig library
    println!("cargo:rustc-link-search=native={}", zig_lib_path.display());
    println!("cargo:rustc-link-lib=static=zig-tls-tunnel");
    
    // Link BoringSSL
    println!("cargo:rustc-link-search=native={}", boringssl_path.display());
    println!("cargo:rustc-link-lib=static=ssl");
    println!("cargo:rustc-link-lib=static=crypto");
    
    // Link C++ (BoringSSL needs it)
    println!("cargo:rustc-link-lib=dylib=stdc++");
    
    // Rerun if libraries change
    println!("cargo:rerun-if-changed={}", zig_lib_path.join("libzig-tls-tunnel.a").display());
    println!("cargo:rerun-if-changed={}", boringssl_path.join("libssl.a").display());
}
