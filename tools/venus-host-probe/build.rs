use std::env;
use std::path::PathBuf;

fn main() {
    let prefix = env::var_os("BRIDGEVM_VENUS_PREFIX")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("BridgeVM/3d/prefix")))
        .expect("BRIDGEVM_VENUS_PREFIX or HOME must be set");
    let lib_dir = prefix.join("lib");
    let lib_dir = lib_dir
        .to_str()
        .expect("virglrenderer prefix lib path must be valid UTF-8");

    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib=dylib=virglrenderer");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{lib_dir}");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("macos") {
        println!("cargo:rustc-link-lib=framework=OpenGL");
    }
}
