//! Bounded live proof that a guest enumerates the BridgeVM PC PCIe identities.

#[cfg(any(test, all(target_os = "macos", target_arch = "aarch64")))]
#[path = "bridgevm_pc_pcie_live/contract.rs"]
mod contract;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "bridgevm_pc_pcie_live/apple.rs"]
mod apple;

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn main() -> Result<(), String> {
    apple::run()
}

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
fn main() {
    eprintln!("BridgeVM PC PCIe probe requires Apple Silicon macOS");
}
