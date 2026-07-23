//! The assembled Path A "QEMU virt" platform, decomposed by responsibility:
//! machine assembly, firmware handoff, MMIO dispatch, interrupt routing, and
//! one module per device family.
mod bootorder;
#[cfg(test)]
mod tests;

mod audio_device;
mod console_device;
mod env_config;
mod firmware_tables;
mod gpu_device;
mod gpu_device_setup;
mod guest_memory;
mod interrupt_routing;
mod machine_assembly;
pub(crate) mod mmio_dispatch;
mod mmio_types;
mod net_backend;
mod net_device;
mod platform_config;
mod snapshot;
mod soc_devices;
mod storage_devices;
mod tpm_devices;
mod xhci_input;

pub(crate) use env_config::*;
pub(crate) use gpu_device_setup::*;
pub use guest_memory::*;
pub use machine_assembly::*;
pub use mmio_types::*;
pub(crate) use net_backend::*;
pub use platform_config::*;
pub use storage_devices::*;
pub use xhci_input::*;
