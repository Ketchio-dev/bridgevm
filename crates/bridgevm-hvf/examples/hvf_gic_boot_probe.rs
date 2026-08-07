// allow: SIZE_OK - Task 5q boot probe harness is a legacy monolithic surface carried to preserve validated HVF/PCIe evidence; full modular split is separate work.
//! Bounded probe: load the stock ArmVirtQemu firmware on the Path A platform with
//! Apple's in-kernel GICv3 (`hv_gic_create`) wired in, and see how far past the
//! previous GIC system-register trap it boots. Captures PL011 serial and records
//! unmodelled MMIO. The GIC distributor/redistributor MMIO and ICC_* system
//! registers are handled in-kernel by Apple, so they no longer trap to us.
//!
//! Build, ad-hoc sign, run (needs `com.apple.security.hypervisor`):
//!   cargo build -p bridgevm-hvf --example hvf_gic_boot_probe
//!   codesign --sign - --entitlements hv.entitlements --force target/debug/examples/hvf_gic_boot_probe && target/debug/examples/hvf_gic_boot_probe
//!
//! Optional NVMe media:
//!   BRIDGEVM_NVME_DISK=/path/to/raw.img target/debug/examples/hvf_gic_boot_probe
//!   BRIDGEVM_NVME_DISK_OUT=/path/to/out.img ...      # snapshot after run
//!   BRIDGEVM_NVME_DISK_WRITABLE=1 ...                # write back to input path
//!
//! Optional installer ISO media (PCI boot media by default):
//!   BRIDGEVM_INSTALLER_ISO=/path/to/windows.iso BRIDGEVM_INSTALLER_ISO_TRANSPORT=mmio ... # transport= legacy virtio-mmio slot 31 fallback
//!   BRIDGEVM_UART_RX=' ' ...                          # preloaded serial input; _ON_CD_PROMPT injects after the cdboot prompt
//!   BRIDGEVM_XHCI_BOOT_KEY_ON_CD_PROMPT=' ' ...        # queue xHCI HID Space after cdboot prompt
//!   BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS='win+r,text:notepad,enter' BRIDGEVM_XHCI_SETUP_INPUT2_ACTIONS='text:g021keys' BRIDGEVM_XHCI_SETUP_INPUT_SERIAL_MARKER='BdsDxe: starting Boot0003' ... # queue guarded setup input actions
//!   BRIDGEVM_RAMFB_SAMPLE_MS=1000,5000,15000 ...       # symmetric elapsed RAMFB checkpoints for no-input/setup-input probes
//!   BRIDGEVM_RAMFB_SAMPLE_UNTIL_COMPLETE=1 ...         # proof mode: observe UEFI shell but continue until RAMFB samples complete
//!   BRIDGEVM_UART_RX_ON_SERIAL_MARKER=' ' BRIDGEVM_UART_RX_SERIAL_MARKER='BdsDxe: starting Boot0001' ...  # serial-marker-gated UART injection
//!   BRIDGEVM_VIRTIO_CONSOLE=1 BRIDGEVM_VIRTIO_CONSOLE_TEST=1 BRIDGEVM_VIRTIO_CONSOLE_CMDS='whoami|ver' BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS=180000 ... # drive bvagent.ps1 over virtio-console
//!   BRIDGEVM_HDA=1 BRIDGEVM_HDA_COREAUDIO=1 ...       # play guest HDA PCM on Mac speakers
//!
//! Optional QEMU-style Linux direct boot:
//!   BRIDGEVM_LINUX_KERNEL=/path/to/Image BRIDGEVM_LINUX_CMDLINE='console=ttyAMA0' ... # BRIDGEVM_LINUX_INITRD optional
//!   BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX=0 ...           # keep running after early Linux logs
//!   BRIDGEVM_RAM_MIB=4096 ...                         # Windows-scale RAM experiments
//!
//! Optional writable UEFI vars:
//!   BRIDGEVM_AARCH64_UEFI_VARS=/path/to/vars.fd ...
//!   BRIDGEVM_AARCH64_UEFI_VARS_OUT=/path/to/vars-out.fd ...
//!   BRIDGEVM_AARCH64_UEFI_VARS_WRITABLE=1 ...        # write back to vars path
//!   BRIDGEVM_SWTPM_DATA_SOCKET=/path/to/swtpm.sock ... # opt-in TPM2 TIS backend; supervisor owns swtpm lifecycle

use std::alloc::{alloc_zeroed, Layout};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc, Condvar, Mutex, MutexGuard, OnceLock, TryLockError,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};
use std::{collections::BTreeMap, os::raw::c_void};
use std::{process::ExitCode, ptr::null_mut};

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

// Every module lives in the hvf_gic_boot_probe/ directory beside this
// file. Cargo resolves example submodules relative to the examples/
// root, so each needs an explicit #[path]; this macro writes that
// attribute once instead of two lines per module.
macro_rules! probe_modules {
    ($($path:literal => $name:ident),+ $(,)?) => {
        $(
            #[path = $path]
            mod $name;
        )+
    };
}
probe_modules!(
    "hvf_gic_boot_probe/agent_console.rs" => agent_console,
    "hvf_gic_boot_probe/arm64_trace.rs" => arm64_trace,
    "hvf_gic_boot_probe/checkpoint_glue.rs" => checkpoint_glue,
    "hvf_gic_boot_probe/device_shape.rs" => device_shape,
    "hvf_gic_boot_probe/mmio_trace.rs" => mmio_trace,
    "hvf_gic_boot_probe/nvme_trace.rs" => nvme_trace,
    "hvf_gic_boot_probe/nvme_storage_effect.rs" => nvme_storage_effect,
    "hvf_gic_boot_probe/pcie_mmio_trace.rs" => pcie_mmio_trace,
    "hvf_gic_boot_probe/storage_effect_receipt.rs" => storage_effect_receipt,
    "hvf_gic_boot_probe/pcie_ecam_trace.rs" => pcie_ecam_trace,
    "hvf_gic_boot_probe/pe_trace.rs" => pe_trace,
    "hvf_gic_boot_probe/ramfb_dump.rs" => ramfb_dump,
    "hvf_gic_boot_probe/ramfb_sample_loop.rs" => ramfb_sample_loop,
    "hvf_gic_boot_probe/live_display_export.rs" => live_display_export,
    "hvf_gic_boot_probe/live_input.rs" => live_input,
    "hvf_gic_boot_probe/hda_coreaudio.rs" => hda_coreaudio,
    "hvf_gic_boot_probe/vblank_wake.rs" => vblank_wake,
    "hvf_gic_boot_probe/kd_serial_bridge.rs" => kd_serial_bridge,
    "hvf_gic_boot_probe/serial_input.rs" => serial_input,
    "hvf_gic_boot_probe/xhci_hid_input.rs" => xhci_hid_input,
    "hvf_gic_boot_probe/watchpoint_setup.rs" => watchpoint_setup,
    "hvf_gic_boot_probe/xhci_trace.rs" => xhci_trace,
    "hvf_gic_boot_probe/boot_telemetry.rs" => boot_telemetry,
    "hvf_gic_boot_probe/exception_trace.rs" => exception_trace,
    "hvf_gic_boot_probe/gic_snapshot.rs" => gic_snapshot,
    "hvf_gic_boot_probe/gic_irq_state.rs" => gic_irq_state,
    "hvf_gic_boot_probe/gpu_shm_bar2.rs" => gpu_shm_bar2,
    "hvf_gic_boot_probe/gpu_shm_setup.rs" => gpu_shm_setup,
    "hvf_gic_boot_probe/guest_diagnostics.rs" => guest_diagnostics,
    "hvf_gic_boot_probe/guest_memory.rs" => guest_memory,
    "hvf_gic_boot_probe/host_support.rs" => host_support,
    "hvf_gic_boot_probe/hvf_abi.rs" => hvf_abi,
    "hvf_gic_boot_probe/interrupt_delivery.rs" => interrupt_delivery,
    "hvf_gic_boot_probe/nvme_persist.rs" => nvme_persist,
    "hvf_gic_boot_probe/probe_env.rs" => probe_env,
    "hvf_gic_boot_probe/psci_adapter.rs" => psci_adapter,
    "hvf_gic_boot_probe/reboot_watchdog.rs" => reboot_watchdog,
    "hvf_gic_boot_probe/secondary_vcpu.rs" => secondary_vcpu,
    "hvf_gic_boot_probe/smp_trace.rs" => smp_trace,
    "hvf_gic_boot_probe/storage_reporting.rs" => storage_reporting,
    "hvf_gic_boot_probe/trng_dispatch.rs" => trng_dispatch,
    "hvf_gic_boot_probe/vcpu_coordination.rs" => vcpu_coordination,
    "hvf_gic_boot_probe/vcpu_debug.rs" => vcpu_debug,
    "hvf_gic_boot_probe/wake_coordinator.rs" => wake_coordinator,
    "hvf_gic_boot_probe/wfi_diagnostics.rs" => wfi_diagnostics,
    "hvf_gic_boot_probe/boot_media_setup.rs" => boot_media_setup,
    "hvf_gic_boot_probe/final_report.rs" => final_report,
    "hvf_gic_boot_probe/hvf_setup.rs" => hvf_setup,
    "hvf_gic_boot_probe/probe_config.rs" => probe_config,
    "hvf_gic_boot_probe/probe_runtime.rs" => probe_runtime,
    "hvf_gic_boot_probe/probe_setup.rs" => probe_setup,
);
use agent_console::AgentConsoleHarness;
use arm64_trace::print_translated_instruction_words;
use live_display_export::LiveDisplayExporter;
use mmio_trace::{print_mmio_traces, record_mmio_trace, MmioTrace};
use nvme_trace::print_nvme_command_trace;
use pcie_ecam_trace::{PcieEcamAccess, RecentPcieEcam};
use pcie_mmio_trace::{
    targetless_xhci_trace_context, PcieMmioEventInput, PcieTraceTarget, RecentMmio,
};
use pe_trace::{print_frame_chain, print_pe_owner, print_translated_pe_owner, translated_ipa};
use ramfb_sample_loop::{RamfbSampleLoop, RamfbSampleShellAction};

#[cfg(target_os = "macos")]
use live_input::LiveInputController;
use serial_input::SerialTriggeredUartInput;
use xhci_hid_input::{
    print_hid_semantic_summary, print_pointer_input_rejection, print_setup_input_rejection,
    SetupInputHostWake, XhciHidBootKeyTrigger, XhciPointerInputTrigger, XhciSetupInputTrigger,
};
use xhci_trace::XhciBringupTrace;

pub(crate) use boot_telemetry::*;
pub(crate) use exception_trace::*;
pub(crate) use guest_diagnostics::*;
pub(crate) use guest_memory::*;
pub(crate) use host_support::*;
pub(crate) use hvf_abi::*;
pub(crate) use interrupt_delivery::*;
pub(crate) use nvme_persist::*;
pub(crate) use probe_env::*;
pub(crate) use psci_adapter::*;
pub(crate) use reboot_watchdog::*;
pub(crate) use secondary_vcpu::*;
pub(crate) use smp_trace::*;
pub(crate) use storage_reporting::*;
pub(crate) use vcpu_coordination::*;
pub(crate) use vcpu_debug::*;
pub(crate) use wake_coordinator::*;
#[path = "hvf_gic_boot_probe/wake_coordinator/cancel_stop.rs"]
mod cancel_stop;
pub(crate) use cancel_stop::{cancel_stop_reason, stall_gic_report};
#[path = "hvf_gic_boot_probe/wake_coordinator/stall_report.rs"]
mod stall_report;
pub(crate) use stall_report::report_stall_diagnostics;
pub(crate) use wfi_diagnostics::*;

fn probe_exit_code(
    fatal_vcpu_run_error: bool,
    fatal_reset_error: bool,
    exit_for_recreate: bool,
) -> ExitCode {
    if fatal_vcpu_run_error || fatal_reset_error {
        ExitCode::FAILURE
    } else if exit_for_recreate {
        // The supervisor contract: 42 = the guest requested SYSTEM_RESET
        // and this process ended cleanly for recreation, storage synced.
        ExitCode::from(RESET_EXIT_CODE)
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
