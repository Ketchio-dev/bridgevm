fn main() {
    println!("cargo:rustc-check-cfg=cfg(bridgevm_non_debug_profile)");
    println!("cargo:rerun-if-env-changed=PROFILE");
    if std::env::var("PROFILE").as_deref() != Ok("debug") {
        println!("cargo:rustc-cfg=bridgevm_non_debug_profile");
    }
}
