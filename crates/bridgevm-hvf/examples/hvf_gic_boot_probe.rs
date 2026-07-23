// allow: SIZE_OK - Task 5q boot probe harness is a legacy monolithic surface carried to preserve validated HVF/PCIe evidence; full modular split is separate work.
//! Bounded probe: load the stock ArmVirtQemu firmware on the Path A platform with
//! Apple's in-kernel GICv3 (`hv_gic_create`) wired in, and see how far past the
//! previous GIC system-register trap it boots. Captures PL011 serial and records
//! unmodelled MMIO. The GIC distributor/redistributor MMIO and ICC_* system
//! registers are handled in-kernel by Apple, so they no longer trap to us.
//!
//! Build, ad-hoc sign, run (needs `com.apple.security.hypervisor`):
//!   cargo build -p bridgevm-hvf --example hvf_gic_boot_probe
//!   codesign --sign - --entitlements hv.entitlements --force target/debug/examples/hvf_gic_boot_probe
//!   target/debug/examples/hvf_gic_boot_probe
//!
//! Optional NVMe media:
//!   BRIDGEVM_NVME_DISK=/path/to/raw.img target/debug/examples/hvf_gic_boot_probe
//!   BRIDGEVM_NVME_DISK_OUT=/path/to/out.img ...      # snapshot after run
//!   BRIDGEVM_NVME_DISK_WRITABLE=1 ...                # write back to input path
//!
//! Optional installer ISO media (PCI boot media by default):
//!   BRIDGEVM_INSTALLER_ISO=/path/to/windows.iso ...
//!   BRIDGEVM_INSTALLER_ISO_TRANSPORT=mmio ...          # legacy virtio-mmio slot 31 fallback
//!   BRIDGEVM_UART_RX=' ' ...                          # preloaded serial input bytes
//!   BRIDGEVM_UART_RX_ON_CD_PROMPT=' ' ...             # inject after cdboot prompt is printed
//!   BRIDGEVM_XHCI_BOOT_KEY_ON_CD_PROMPT=' ' ...        # queue xHCI HID Space after cdboot prompt
//!   BRIDGEVM_XHCI_BOOT_KEY_ON_SERIAL_MARKER=' ' BRIDGEVM_XHCI_BOOT_KEY_SERIAL_MARKER='BdsDxe: starting Boot0001' ... # debug frontier xHCI Space trigger; CD prompt remains separate
//!   BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS='win+r,text:notepad,enter' BRIDGEVM_XHCI_SETUP_INPUT2_ACTIONS='text:g021keys' BRIDGEVM_XHCI_SETUP_INPUT_SERIAL_MARKER='BdsDxe: starting Boot0003' ... # queue guarded setup input actions
//!   BRIDGEVM_RAMFB_SAMPLE_MS=1000,5000,15000 ...       # symmetric elapsed RAMFB checkpoints for no-input/setup-input probes
//!   BRIDGEVM_RAMFB_SAMPLE_UNTIL_COMPLETE=1 ...         # proof mode: observe UEFI shell but continue until RAMFB samples complete
//!   BRIDGEVM_UART_RX_ON_SERIAL_MARKER=' ' BRIDGEVM_UART_RX_SERIAL_MARKER='BdsDxe: starting Boot0001' ...
//!   BRIDGEVM_VIRTIO_CONSOLE=1 BRIDGEVM_VIRTIO_CONSOLE_TEST=1 ... # drive bvagent.ps1 over virtio-console
//!   BRIDGEVM_VIRTIO_CONSOLE_CMDS='whoami|ver|ipconfig' ...
//!   BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=180000 ...
//!   BRIDGEVM_HDA=1 BRIDGEVM_HDA_COREAUDIO=1 ...       # play guest HDA PCM on Mac speakers
//!
//! Optional QEMU-style Linux direct boot:
//!   BRIDGEVM_LINUX_KERNEL=/path/to/Image ...
//!   BRIDGEVM_LINUX_INITRD=/path/to/initrd.gz ...      # optional
//!   BRIDGEVM_LINUX_CMDLINE='console=ttyAMA0 acpi=force' ...
//!   BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX=0 ...           # keep running after early Linux logs
//!   BRIDGEVM_RAM_MIB=4096 ...                         # Windows-scale RAM experiments
//!
//! Optional writable UEFI vars:
//!   BRIDGEVM_AARCH64_UEFI_VARS=/path/to/vars.fd ...
//!   BRIDGEVM_AARCH64_UEFI_VARS_OUT=/path/to/vars-out.fd ...
//!   BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1 ...        # write back to vars path
//!   BRIDGEVM_SWTPM_DATA_SOCKET=/path/to/swtpm.sock ... # opt-in TPM2 TIS backend; supervisor owns swtpm lifecycle

use std::alloc::{alloc_zeroed, Layout};
use std::collections::BTreeMap;
use std::os::raw::c_void;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::ptr::null_mut;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex, MutexGuard, OnceLock, TryLockError,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::media::{
    read_bounded_file, InstallerIsoTransport, MediaWrite, MediaWriteKind, VirtBootMediaConfig,
    WritableMedia,
};
use bridgevm_hvf::msix::MsixMessage;
use bridgevm_hvf::net_nat::NatStats;
use bridgevm_hvf::platform_virt::{
    MmioOp, MmioOutcome, MmioPostDrain, VirtPlatform, VirtPlatformConfig,
};
use bridgevm_hvf::ramfb::{RamfbConfig, RamfbSnapshot, RamfbSnapshotError, RamfbSnapshotSummary};
use bridgevm_hvf::stage1::{self, Stage1Context, Stage1WalkStep};
use bridgevm_hvf::tpm_tis::{SwtpmUnixBackend, Tpm2Backend};
use bridgevm_hvf::virtio_blk::{VirtioBlockRequestTrace, VirtioMmioBlockStats, INSTALLER_ISO_SLOT};
use bridgevm_hvf::virtio_gpu_3d::GpuShmMapPort;

#[path = "hvf_gic_boot_probe/agent_console.rs"]
mod agent_console;
use agent_console::AgentConsoleHarness;
#[path = "hvf_gic_boot_probe/arm64_trace.rs"]
mod arm64_trace;
#[path = "hvf_gic_boot_probe/checkpoint_glue.rs"]
mod checkpoint_glue;
use arm64_trace::print_translated_instruction_words;
#[path = "hvf_gic_boot_probe/device_shape.rs"]
mod device_shape;
#[path = "hvf_gic_boot_probe/mmio_trace.rs"]
mod mmio_trace;
use mmio_trace::{print_mmio_traces, record_mmio_trace, MmioTrace};
#[path = "hvf_gic_boot_probe/nvme_trace.rs"]
mod nvme_trace;
use nvme_trace::print_nvme_command_trace;
#[path = "hvf_gic_boot_probe/nvme_storage_effect.rs"]
mod nvme_storage_effect;
#[path = "hvf_gic_boot_probe/pcie_mmio_trace.rs"]
mod pcie_mmio_trace;
#[path = "hvf_gic_boot_probe/storage_effect_receipt.rs"]
mod storage_effect_receipt;
use pcie_mmio_trace::{
    targetless_xhci_trace_context, PcieMmioEventInput, PcieTraceTarget, RecentMmio,
};
#[path = "hvf_gic_boot_probe/pcie_ecam_trace.rs"]
mod pcie_ecam_trace;
use pcie_ecam_trace::{PcieEcamAccess, RecentPcieEcam};
#[path = "hvf_gic_boot_probe/pe_trace.rs"]
mod pe_trace;
use pe_trace::{print_frame_chain, print_pe_owner, print_translated_pe_owner, translated_ipa};
#[path = "hvf_gic_boot_probe/ramfb_dump.rs"]
mod ramfb_dump;
#[path = "hvf_gic_boot_probe/ramfb_sample_loop.rs"]
mod ramfb_sample_loop;
use ramfb_sample_loop::{RamfbSampleLoop, RamfbSampleShellAction};
#[path = "hvf_gic_boot_probe/live_display_export.rs"]
mod live_display_export;
use live_display_export::LiveDisplayExporter;
#[path = "hvf_gic_boot_probe/live_input.rs"]
mod live_input;

#[cfg(target_os = "macos")]
#[path = "hvf_gic_boot_probe/hda_coreaudio.rs"]
mod hda_coreaudio;

#[path = "hvf_gic_boot_probe/vblank_wake.rs"]
mod vblank_wake;

#[path = "hvf_gic_boot_probe/kd_serial_bridge.rs"]
mod kd_serial_bridge;
use live_input::LiveInputController;
#[path = "hvf_gic_boot_probe/serial_input.rs"]
mod serial_input;
use serial_input::SerialTriggeredUartInput;
#[path = "hvf_gic_boot_probe/xhci_hid_input.rs"]
mod xhci_hid_input;
use xhci_hid_input::{
    print_hid_semantic_summary, print_pointer_input_rejection, print_setup_input_rejection,
    SetupInputHostWake, XhciHidBootKeyTrigger, XhciPointerInputTrigger, XhciSetupInputTrigger,
};
#[path = "hvf_gic_boot_probe/watchpoint_setup.rs"]
mod watchpoint_setup;
#[path = "hvf_gic_boot_probe/xhci_trace.rs"]
mod xhci_trace;
use xhci_trace::XhciBringupTrace;

#[path = "hvf_gic_boot_probe/boot_telemetry.rs"]
mod boot_telemetry;
#[path = "hvf_gic_boot_probe/exception_trace.rs"]
mod exception_trace;
#[path = "hvf_gic_boot_probe/gpu_shm_setup.rs"]
mod gpu_shm_setup;
#[path = "hvf_gic_boot_probe/guest_diagnostics.rs"]
mod guest_diagnostics;
#[path = "hvf_gic_boot_probe/guest_memory.rs"]
mod guest_memory;
#[path = "hvf_gic_boot_probe/host_support.rs"]
mod host_support;
#[path = "hvf_gic_boot_probe/hvf_abi.rs"]
mod hvf_abi;
#[path = "hvf_gic_boot_probe/interrupt_delivery.rs"]
mod interrupt_delivery;
#[path = "hvf_gic_boot_probe/probe_env.rs"]
mod probe_env;
#[path = "hvf_gic_boot_probe/reboot_watchdog.rs"]
mod reboot_watchdog;
#[path = "hvf_gic_boot_probe/secondary_vcpu.rs"]
mod secondary_vcpu;
#[path = "hvf_gic_boot_probe/smp_trace.rs"]
mod smp_trace;
#[path = "hvf_gic_boot_probe/storage_reporting.rs"]
mod storage_reporting;
#[path = "hvf_gic_boot_probe/vcpu_coordination.rs"]
mod vcpu_coordination;
#[path = "hvf_gic_boot_probe/vcpu_debug.rs"]
mod vcpu_debug;
#[path = "hvf_gic_boot_probe/wfi_diagnostics.rs"]
mod wfi_diagnostics;
pub(crate) use boot_telemetry::*;
pub(crate) use exception_trace::*;
pub(crate) use guest_diagnostics::*;
pub(crate) use guest_memory::*;
pub(crate) use host_support::*;
pub(crate) use hvf_abi::*;
pub(crate) use interrupt_delivery::*;
pub(crate) use probe_env::*;
pub(crate) use reboot_watchdog::*;
pub(crate) use secondary_vcpu::*;
pub(crate) use smp_trace::*;
pub(crate) use storage_reporting::*;
pub(crate) use vcpu_coordination::*;
pub(crate) use vcpu_debug::*;
pub(crate) use wfi_diagnostics::*;

#[path = "hvf_gic_boot_probe/boot_media_setup.rs"]
mod boot_media_setup;
#[path = "hvf_gic_boot_probe/final_report.rs"]
mod final_report;
#[path = "hvf_gic_boot_probe/hvf_setup.rs"]
mod hvf_setup;
#[path = "hvf_gic_boot_probe/probe_config.rs"]
mod probe_config;
#[path = "hvf_gic_boot_probe/probe_runtime.rs"]
mod probe_runtime;
#[path = "hvf_gic_boot_probe/probe_setup.rs"]
mod probe_setup;

fn probe_exit_code(fatal_vcpu_run_error: bool, fatal_reset_error: bool) -> ExitCode {
    if fatal_vcpu_run_error || fatal_reset_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

fn main() -> ExitCode {
    if env_flag("BRIDGEVM_PROBE_PRINT_CAPABILITIES") {
        println!("BridgeVM HVF probe build capabilities");
        println!("virtio_gpu_3d_compiled={}", cfg!(feature = "venus"));
        ExitCode::SUCCESS
    } else {
        probe_runtime::run()
    }
}
