//! Live BridgeVM Virtual ARM PC proof of BDS loading BOOTAA64.EFI from an
//! NVMe GPT/FAT ESP and entering code after ExitBootServices.

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "bridgevm_pc_boot_live/apple.rs"]
mod apple;

#[cfg(all(test, not(all(target_os = "macos", target_arch = "aarch64"))))]
#[path = "bridgevm_pc_boot_live/result.rs"]
mod result;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), String> {
    apple::run()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("BridgeVM PC BDS boot probe requires Apple Silicon macOS");
}
