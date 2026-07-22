use std::{
    any::Any,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

// Modules of the "QEMU virt contract" path (Path A). New platform code lands in
// dedicated files like these rather than growing the legacy probe monolith
// below. See docs/hvf-windows-engine-strategy.md and
// docs/hvf-windows-platform-contract-gap.md.
pub mod acpi;
pub mod checkpoint;
pub mod dtb;
pub mod fwcfg;
pub mod hda;
pub mod machine;
pub mod media;
pub mod msix;
pub mod net_nat;
pub mod nvme;
pub mod pcie;
pub mod pflash;
pub mod pl011;
pub mod pl031;
pub mod platform_virt;
pub mod ramfb;
pub mod smbios;
pub mod stage1;
pub mod tpm_ppi;
pub mod tpm_tis;
#[cfg(feature = "venus")]
pub mod venus_backend;
pub mod virtio_blk;
pub mod virtio_console;
pub mod virtio_gpu;
pub mod virtio_gpu_3d;
pub mod virtio_gpu_3d_preflight;
mod virtio_gpu_trace;
pub mod virtio_net;
mod windows_arm_xhci_hid_boot_key_probe;
pub mod xhci;

// Extracted from the legacy probe monolith below (see
// docs/hvf-lib-refactor-extraction-plan.md). These stay private and are
// re-exported explicitly so the crate-root public surface is unchanged.
mod machine_plan;
mod no_qemu_plan;
mod probe_mmio;
mod probes;
mod support;

// Glob re-export preserves each probe item's original visibility, keeping the
// crate-root public surface identical.
pub use probes::*;
mod windows_arm;

// Glob re-export preserves each moved item's original visibility, so the
// public surface is byte-identical and crate-internal helpers stay internal.
pub use windows_arm::*;

// Crate-internal synthetic MMIO harness. Glob import keeps the (formerly
// file-local) names resolving unqualified; every item is pub(crate), so this
// does not widen the public API.
use probe_mmio::*;

pub use machine_plan::{
    build_windows_11_arm_hvf_machine_plan, plan_windows_11_arm_hvf_machine,
    windows_11_arm_hvf_machine_devices, HvfDevicePlan, HvfMachinePlan, HvfMachinePlanOptions,
    HvfMemoryRegionPlan, HvfVcpuLifecyclePlan,
};
pub use no_qemu_plan::{
    plan_windows_11_arm_no_qemu, windows_11_arm_no_qemu_vmm_gates, WindowsArmNoQemuPlan,
    WindowsArmVmmGate,
};
pub use support::{detect_hvf_support, HvfSupport, WindowsArmVmmGateStatus};
pub(crate) use support::{
    hv_exit_reason_name, hv_return_name, HV_EXIT_REASON_CANCELED_VALUE,
    HV_EXIT_REASON_EXCEPTION_VALUE, HV_EXIT_REASON_VTIMER_ACTIVATED_VALUE, HV_SUCCESS_VALUE,
};

pub use virtio_gpu_3d_preflight::{
    probe_virtio_gpu_3d_host_preflight, probe_virtio_gpu_3d_host_preflight_for,
    VirtioGpu3dHostPreflight, VirtioGpu3dHostPreflightProtocol,
};
pub use windows_arm_xhci_hid_boot_key_probe::{
    probe_windows_11_arm_xhci_hid_boot_key_report, WindowsArmXhciHidBootKeyReportProbe,
};

#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
#[path = "platform/apple.rs"]
mod platform;

#[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
#[path = "platform/unsupported.rs"]
mod platform;
