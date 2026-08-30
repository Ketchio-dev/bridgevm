//! Bounded live proof that an EL1 guest can read BridgeVM boot-info v1 from
//! the independent board GPA. This does not boot firmware or Windows.

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[path = "bridgevm_pc_boot_info_live/guest.rs"]
mod guest;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "bridgevm_pc_boot_info_live/apple.rs"]
mod apple;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), String> {
    apple::run()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("BridgeVM Virtual ARM PC boot-info probe requires Apple Silicon macOS");
}
