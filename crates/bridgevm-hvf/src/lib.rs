//! BridgeVM's own Hypervisor.framework VMM for Windows on Arm.
//!
//! Implements the virtual machine directly against Apple's Hypervisor.framework:
//! vCPU execution, the guest memory map, firmware and device tables, and the
//! virtio, NVMe, xHCI, GPU, TPM and network devices a Windows guest expects. This
//! is a from-scratch Rust implementation rather than a wrapper around an existing
//! VMM, which is why the device models and boot contract live here in full.

// See docs/decisions/hvf-windows-engine-strategy.md and
// docs/reference/hvf-windows-platform-contract-gap.md.
pub mod acpi;
pub mod acpi_ged;
pub mod checkpoint;
pub mod disk_import;
pub mod display_fb;
pub mod dtb;
pub mod fwcfg;
mod guest_memory;
pub mod hda;
pub mod host_entropy;
pub mod host_log;
pub mod host_power;
pub mod machine;
pub mod media;
pub mod media_lease;
pub mod media_lease_session;
pub mod mmio_data_abort;
pub mod msix;
pub mod net_nat;
pub mod nvme;
pub mod pcie;
pub mod pflash;
pub mod pl011;
pub mod pl031;
pub mod platform_virt;
pub mod psci;
pub mod ramfb;
pub mod smbios;
pub mod smccc_trng;
pub mod snapshot_pair;
pub mod stage1;
pub mod tpm_ppi;
pub mod tpm_tis;
pub mod uefi_resolution;
#[cfg(feature = "venus")]
pub mod venus_backend;
pub mod virtio_blk;
pub mod virtio_console;
pub mod virtio_gpu;
pub mod virtio_gpu_3d;
pub mod virtio_gpu_3d_preflight;
mod virtio_gpu_trace;
pub mod virtio_net;
mod virtio_queue;
pub mod vm_clock;
pub mod xhci;

pub use virtio_gpu_3d_preflight::{
    probe_virtio_gpu_3d_host_preflight, probe_virtio_gpu_3d_host_preflight_for,
    VirtioGpu3dHostPreflight, VirtioGpu3dHostPreflightProtocol,
};
