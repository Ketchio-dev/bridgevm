//! Bounded live proof that BridgeVM PC dispatches generic RuntimeDxe and keeps
//! its platform tables. This does not claim complete UEFI or Windows boot.

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[path = "bridgevm_pc_dxe_entry_live/contract.rs"]
mod contract;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "bridgevm_pc_reset_vector_live/apple.rs"]
mod apple;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), String> {
    apple::run()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("BridgeVM PC DXE-entry probe requires Apple Silicon macOS");
}
