//! Bounded live proof that BridgeVM PC restores a UEFI variable after an HVF
//! VM destroy/recreate while retaining its runtime and platform tables.

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[path = "bridgevm_pc_dxe_entry_live/contract.rs"]
mod contract;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "bridgevm_pc_dxe_entry_live/apple.rs"]
mod apple;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), String> {
    apple::run()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("BridgeVM PC DXE-entry probe requires Apple Silicon macOS");
}
