use std::{env, path::PathBuf};

fn main() {
    if env::var_os("CARGO_FEATURE_VENUS").is_none() {
        return;
    }

    let prefix = env::var_os("BRIDGEVM_VENUS_PREFIX")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join("BridgeVM/3d/prefix")))
        .expect("BRIDGEVM_VENUS_PREFIX or HOME must be set for the venus feature");
    let lib_dir = prefix.join("lib");
    let lib_dir = lib_dir
        .to_str()
        .expect("virglrenderer prefix lib path must be valid UTF-8");

    println!("cargo:rustc-link-search=native={lib_dir}");
    println!("cargo:rustc-link-lib=dylib=virglrenderer");
    println!("cargo:rustc-link-arg=-Wl,-rpath,{lib_dir}");
}
