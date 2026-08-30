//! Bounded live proof that the BridgeVM Virtual ARM PC reset vector executes
//! from flash offset zero. This does not execute SEC, PEI, DXE or Windows.

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[path = "bridgevm_pc_reset_vector_live/contract.rs"]
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
    eprintln!("BridgeVM Virtual ARM PC reset-vector probe requires Apple Silicon macOS");
}
