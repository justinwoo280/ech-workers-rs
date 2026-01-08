use std::path::PathBuf;

fn main() {
    let zig_lib_path = PathBuf::from("zig-tls-tunnel/zig-out/lib");
    let boringssl_path = PathBuf::from("zig-tls-tunnel/vendor/boringssl/build");
    
    // Link Zig library (~9KB)
    println!("cargo:rustc-link-search=native={}", zig_lib_path.display());
    println!("cargo:rustc-link-lib=static=zig-tls-tunnel");
    
    // Link BoringSSL (~63MB total)
    println!("cargo:rustc-link-search=native={}", boringssl_path.display());
    println!("cargo:rustc-link-lib=static=ssl");
    println!("cargo:rustc-link-lib=static=crypto");
    
    // Link C++ (BoringSSL needs it)
    println!("cargo:rustc-link-lib=dylib=stdc++");
    
    // Rerun if libraries change
    println!("cargo:rerun-if-changed=zig-tls-tunnel/zig-out/lib/libzig-tls-tunnel.a");
    println!("cargo:rerun-if-changed=zig-tls-tunnel/vendor/boringssl/build/libssl.a");
}
