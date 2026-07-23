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
#[path = "hvf_gic_boot_probe/xhci_trace.rs"]
mod xhci_trace;
use xhci_trace::XhciBringupTrace;

#[path = "hvf_gic_boot_probe/boot_telemetry.rs"]
mod boot_telemetry;
#[path = "hvf_gic_boot_probe/exception_trace.rs"]
mod exception_trace;
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

fn main() -> ExitCode {
    if env_flag("BRIDGEVM_PROBE_PRINT_CAPABILITIES") {
        println!("BridgeVM HVF probe build capabilities");
        println!("virtio_gpu_3d_compiled={}", cfg!(feature = "venus"));
        return ExitCode::SUCCESS;
    }

    let mut fatal_vcpu_run_error = false;
    let mut fatal_reset_error = false;
    let media = VirtBootMediaConfig::from_probe_env();
    let smp_cpus = env_u64("BRIDGEVM_SMP_CPUS", 1).clamp(1, machine::MAX_CPUS);
    let smp_cpus = if machine::redist_fits(smp_cpus) {
        smp_cpus
    } else {
        1
    };
    let swtpm_data_socket = std::env::var_os("BRIDGEVM_SWTPM_DATA_SOCKET").map(PathBuf::from);
    let swtpm_control_socket = std::env::var_os("BRIDGEVM_SWTPM_CONTROL_SOCKET").map(PathBuf::from);
    let mut platform_devices = media.platform_devices;
    platform_devices.tpm_tis_present = swtpm_data_socket.is_some();
    let platform_cfg = VirtPlatformConfig {
        fdt: VirtFdtConfig {
            cpu_count: smp_cpus,
            ram_size: media.ram_size,
        },
        devices: platform_devices,
    };
    let ram_size = usize::try_from(media.ram_size).expect("guest RAM size does not fit usize");
    assert!(
        ram_size >= 128 * 1024 * 1024,
        "guest RAM must be at least 128 MiB"
    );
    println!("Guest RAM: {} MiB", media.ram_size / (1024 * 1024));
    println!("SMP CPUs advertised: {smp_cpus}");
    let watchdog_ms = env_u64("BRIDGEVM_BOOT_PROBE_WATCHDOG_MS", WATCHDOG_MS);
    let watchdog_enabled = !env_flag("BRIDGEVM_BOOT_PROBE_WATCHDOG_DISABLED");
    if watchdog_enabled {
        println!("Boot watchdog: {watchdog_ms} ms per boot generation");
    } else {
        println!("Boot watchdog: disabled; guest/user shutdown required");
    }
    let trace_fwcfg = env_flag("BRIDGEVM_TRACE_FWCFG");
    let trace_msix = env_flag("BRIDGEVM_TRACE_MSIX");
    let trace_spi = env_flag("BRIDGEVM_TRACE_SPI");
    let trace_run_loop = env_flag("BRIDGEVM_TRACE_RUN_LOOP");
    let trace_xhci_bringup = env_flag("BRIDGEVM_TRACE_XHCI_BRINGUP");
    let smp_trace_enabled = env_flag("BRIDGEVM_SMP_TRACE");
    let stop_on_linux = env_flag_default("BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX", true);

    unsafe {
        // Create the VM with the max IPA size: the PCIe ECAM sits at 256 GiB,
        // beyond the 36-bit default IPA window.
        let vmcfg = hv_vm_config_create();
        let mut max_ipa = 0u32;
        hv_vm_config_get_max_ipa_size(&mut max_ipa);
        hv_vm_config_set_ipa_size(vmcfg, max_ipa);
        let mut el2_supported = false;
        let el2_supported_status = hv_vm_config_get_el2_supported(&mut el2_supported);
        let mut el2_enabled_before = false;
        let el2_enabled_before_status =
            hv_vm_config_get_el2_enabled(vmcfg, &mut el2_enabled_before);
        let request_el2 = env_flag("BRIDGEVM_ENABLE_EL2");
        let el2_enable_status = if request_el2 && el2_supported_status == 0 && el2_supported {
            hv_vm_config_set_el2_enabled(vmcfg, true)
        } else {
            0
        };
        let mut el2_enabled_after = false;
        let el2_enabled_after_status = hv_vm_config_get_el2_enabled(vmcfg, &mut el2_enabled_after);
        println!(
            "EL2 config: requested={} supported={} status={el2_supported_status:#x}, enabled_before={} status={el2_enabled_before_status:#x}, set_true={el2_enable_status:#x}, enabled_after={} status={el2_enabled_after_status:#x}",
            request_el2, el2_supported, el2_enabled_before, el2_enabled_after
        );
        let vc = hv_vm_create(vmcfg);
        println!("hv_vm_create(ipa={max_ipa}) = {vc:#x}");
        assert_eq!(vc, 0, "hv_vm_create");
        let _vm_guard = HvVmGuard;

        // In-kernel GICv3 must be created after the VM and before any vCPU.
        let gic = hv_gic_config_create();
        assert_eq!(
            hv_gic_config_set_distributor_base(gic, machine::GIC_DIST.base),
            0,
            "set dist base"
        );
        assert_eq!(
            hv_gic_config_set_redistributor_base(gic, machine::GIC_REDIST.base),
            0,
            "set redist base"
        );
        let mut spi_intid_base = 0u32;
        let mut spi_intid_count = 0u32;
        assert_eq!(
            hv_gic_get_spi_interrupt_range(&mut spi_intid_base, &mut spi_intid_count),
            0,
            "get SPI INTID range"
        );
        let msi_intid_base = machine::GIC_MSI_INTID_BASE;
        let msi_intid_count = machine::GIC_MSI_INTID_COUNT;
        assert!(
            msi_intid_base >= spi_intid_base
                && msi_intid_base + msi_intid_count <= spi_intid_base + spi_intid_count,
            "MSI INTID range {msi_intid_base}..{} outside supported SPI INTID range {spi_intid_base}..{}",
            msi_intid_base + msi_intid_count,
            spi_intid_base + spi_intid_count
        );
        assert_eq!(
            hv_gic_config_set_msi_region_base(gic, machine::GIC_ITS.base),
            0,
            "set MSI region base"
        );
        assert_eq!(
            hv_gic_config_set_msi_interrupt_range(gic, msi_intid_base, msi_intid_count),
            0,
            "set MSI INTID range"
        );
        let gic_r = hv_gic_create(gic);
        println!(
            "hv_gic_create = {gic_r:#x} (dist {:#x}, redist {:#x}, msi {:#x}+{:#x}, intids {}..{})",
            machine::GIC_DIST.base,
            machine::GIC_REDIST.base,
            machine::GIC_ITS.base,
            HV_GIC_REG_GICM_SET_SPI_NSR,
            msi_intid_base,
            msi_intid_base + msi_intid_count
        );
        assert_eq!(gic_r, 0, "hv_gic_create");

        map_file(
            &media.firmware_code_path,
            machine::FLASH_CODE.base,
            machine::FLASH_CODE.size as usize,
            HV_MEMORY_READ | HV_MEMORY_EXEC,
        );
        let vars_data = media
            .flash_vars
            .read_bounded(machine::FLASH_VARS.size as usize)
            .unwrap_or_else(|e| panic!("read UEFI vars {}: {e}", media.flash_vars.path.display()));
        let tpm_backend: Option<Box<dyn Tpm2Backend>> =
            swtpm_data_socket
                .as_ref()
                .map(|path| match swtpm_control_socket.as_ref() {
                    Some(control) => {
                        println!(
                            "TPM2 TIS backend: swtpm data socket {} control socket {}",
                            path.display(),
                            control.display()
                        );
                        Box::new(
                            SwtpmUnixBackend::connect_with_control(path, Some(control))
                                .unwrap_or_else(|error| {
                                    panic!(
                                        "connect swtpm data {} control {}: {error}",
                                        path.display(),
                                        control.display()
                                    )
                                }),
                        ) as Box<dyn Tpm2Backend>
                    }
                    None => {
                        println!("TPM2 TIS backend: swtpm data socket {}", path.display());
                        Box::new(SwtpmUnixBackend::connect(path).unwrap_or_else(|error| {
                            panic!("connect swtpm {}: {error}", path.display())
                        })) as Box<dyn Tpm2Backend>
                    }
                });
        let mut platform = VirtPlatform::new_with_config_and_tpm_backend(platform_cfg, tpm_backend);
        if env_flag("BRIDGEVM_HDA_COREAUDIO") {
            if !platform_cfg.devices.hda_present {
                eprintln!("BRIDGEVM_HDA_COREAUDIO ignored because BRIDGEVM_HDA is not enabled");
            } else {
                #[cfg(target_os = "macos")]
                {
                    let sink = hda_coreaudio::CoreAudioPcmSink::new()
                        .unwrap_or_else(|error| panic!("initialize HDA CoreAudio output: {error}"));
                    platform.set_hda_pcm_sink(Some(Box::new(sink)));
                    println!("HDA CoreAudio output: s16le 48000 Hz stereo, enabled");
                }
                #[cfg(not(target_os = "macos"))]
                panic!("BRIDGEVM_HDA_COREAUDIO is only supported on macOS");
            }
        }
        if platform_cfg.devices.ramfb_present {
            println!("ramfb fw_cfg: enabled");
        } else if env_flag("BRIDGEVM_RAMFB") {
            println!("ramfb fw_cfg: disabled by BRIDGEVM_DISABLE_RAMFB_DEVICE");
        } else {
            println!("ramfb fw_cfg: disabled");
        }
        if !platform_cfg.devices.xhci_present {
            println!("qemu-xhci: disabled by BRIDGEVM_DISABLE_XHCI");
        }
        if !platform_cfg.devices.virtio_boot_media_present {
            println!("virtio installer ISO surfaces: disabled by BRIDGEVM_DISABLE_VIRTIO_ISO");
        }
        let xhci_report_interval = parse_xhci_report_interval_env();
        platform.set_xhci_report_interval(xhci_report_interval);
        if xhci_report_interval.is_zero() {
            println!("xHCI HID report pacing: disabled (BRIDGEVM_XHCI_REPORT_INTERVAL_MS=0)");
        } else {
            println!(
                "xHCI HID report pacing: {} ms between reports",
                xhci_report_interval.as_millis()
            );
        }

        let ram_layout = Layout::from_size_align(ram_size, 0x1_0000).unwrap();
        let ram = alloc_zeroed(ram_layout);
        let boot_dtb = platform.dtb().to_vec();
        assert!(boot_dtb.len() < ram_size, "DTB must fit in guest RAM");
        assert_eq!(
            hv_vm_map(
                ram as *mut c_void,
                machine::RAM_BASE,
                ram_size,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC
            ),
            0,
            "map ram"
        );

        let mut vcpu: HvVcpuT = 0;
        let mut exit: *mut HvVcpuExit = null_mut();
        assert_eq!(
            hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
            0,
            "hv_vcpu_create"
        );
        let _vcpu_guard = HvVcpuGuard { vcpu };

        // Hardware watchpoint on a firmware address (default the poll target
        // 0x5ffdf798): route guest debug exceptions to the host, then arm
        // DBGWVR0/DBGWCR0 for stores. Unlike a read-only page, this traps only the
        // exact address with no emulation. Opt-in via BRIDGEVM_WATCH=1 (or
        // BRIDGEVM_WATCH=0x... for a different address) since single-stepping each
        // store perturbs boot timing.
        let watch_addr: Option<u64> = std::env::var("BRIDGEVM_WATCH").ok().map(|s| {
            let s = s.trim();
            if s == "1" {
                WATCH_TARGET
            } else {
                parse_u64(s).unwrap_or(WATCH_TARGET)
            }
        });
        let watch_target = watch_addr.unwrap_or(WATCH_TARGET);
        platform.load_flash_vars(&vars_data);
        if let Some(nvme) = media.nvme_disk.as_ref() {
            platform
                .attach_nvme_raw_file(&nvme.path, nvme.write_back)
                .unwrap_or_else(|e| panic!("attach NVMe disk {}: {e}", nvme.path.display()));
            println!(
                "NVMe raw disk attached: {} ({} bytes, write_back={})",
                nvme.path.display(),
                platform.nvme_disk_len(),
                nvme.write_back
            );
            println!(
                "NVMe data path: {}",
                if platform.nvme_direct_dma_enabled() {
                    "direct-dma"
                } else {
                    "buffered (BRIDGEVM_NVME_BUFFERED_IO=1)"
                }
            );
        }
        if let Some(target) = media.nvme_target.as_ref() {
            platform
                .attach_nvme_second_namespace_raw_file(&target.path, target.write_back)
                .unwrap_or_else(|e| {
                    panic!("attach NVMe target (NSID 2) {}: {e}", target.path.display())
                });
            println!(
                "NVMe target namespace (NSID 2) attached: {} (write_back={})",
                target.path.display(),
                target.write_back
            );
        }
        let mut pci_installer_iso_attached = false;
        let mut legacy_mmio_installer_iso_attached = false;
        if let Some(path) = media.installer_iso_path.as_ref() {
            match media.installer_iso_transport {
                InstallerIsoTransport::Pci if platform_cfg.devices.virtio_boot_media_present => {
                    platform.attach_pci_boot_media(path).unwrap_or_else(|e| {
                        panic!("attach PCI installer ISO {}: {e}", path.display())
                    });
                    pci_installer_iso_attached = true;
                    println!(
                        "Installer ISO attached on PCI boot media 00:03.0: {}",
                        path.display()
                    );
                }
                InstallerIsoTransport::Pci => {
                    println!(
                        "Installer ISO PCI boot media disabled; not attaching {}",
                        path.display()
                    );
                }
                InstallerIsoTransport::Mmio if platform_cfg.devices.legacy_virtio_mmio_present => {
                    platform.attach_virtio_iso(path).unwrap_or_else(|e| {
                        panic!("attach legacy MMIO installer ISO {}: {e}", path.display())
                    });
                    legacy_mmio_installer_iso_attached = true;
                    println!(
                        "Installer ISO attached on legacy virtio-mmio slot {INSTALLER_ISO_SLOT}: {}",
                        path.display()
                    );
                }
                InstallerIsoTransport::Mmio => {
                    println!(
                        "Installer ISO legacy virtio-mmio disabled; not attaching {}",
                        path.display()
                    );
                }
            }
        }
        device_shape::print_device_shape(
            platform_cfg.devices,
            pci_installer_iso_attached,
            legacy_mmio_installer_iso_attached,
            platform.nvme_disk_len(),
        );
        if let Some(linux) = media.linux_boot.as_ref() {
            let kernel = linux.read_kernel_bounded(ram_size).unwrap_or_else(|e| {
                panic!("read Linux kernel {}: {e}", linux.kernel_path.display())
            });
            let initrd = linux
                .read_initrd_bounded(ram_size)
                .unwrap_or_else(|e| panic!("read Linux initrd: {e}"));
            println!(
                "Linux kernel loaded: {} ({} bytes)",
                linux.kernel_path.display(),
                kernel.len()
            );
            if let Some(path) = linux.initrd_path.as_ref() {
                println!(
                    "Linux initrd loaded: {} ({} bytes)",
                    path.display(),
                    initrd.as_ref().map_or(0, Vec::len)
                );
            }
            println!(
                "Linux cmdline loaded: {:?} ({} bytes including NUL)",
                linux.cmdline,
                linux.cmdline_bytes().len()
            );
            platform.set_linux_boot_blobs(kernel, initrd, linux.cmdline_bytes());
        }
        let mut guest_ram = MappedRam {
            base: machine::RAM_BASE,
            ptr: ram,
            len: ram_size,
        };
        reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb);
        let reboot_plan = RebootPlan::from_env();
        println!("PSCI SYSTEM_RESET max reboots: {}", reboot_plan.max_reboots);
        let watchdog_generation = Arc::new(AtomicU64::new(0));
        let mut reboot_count = 0u64;
        let mut resets_dumped = 0u64;
        reset_vcpu_for_boot(vcpu);
        arm_watchpoint_for_boot(vcpu, watch_addr);
        checkpoint_glue::restore_if_requested(
            &[vcpu],
            std::slice::from_raw_parts_mut(ram, ram_size),
            &mut platform,
        )
        .unwrap_or_else(|error| panic!("restore VM checkpoint: {error}"));
        let hv_gpu_shm_state = Arc::new(Mutex::new(HvGpuShmMapState::default()));
        let installed_hv_gpu_shm_port =
            platform.set_virtio_gpu_shm_map_port(Box::new(HvGpuShmMapPort {
                state: Arc::clone(&hv_gpu_shm_state),
            }));
        if installed_hv_gpu_shm_port {
            println!("virtio-gpu host-visible shm map port: hv_vm_map enabled");
        }
        let platform = Arc::new(Mutex::new(platform));
        // Probe-lifetime instance, deliberately OUTSIDE the reboot loop: its
        // ticker thread keeps firing across guest reboots, and the SAME fired
        // flag must be the one each boot generation's exit dispatcher consumes.
        // (A per-boot instance turned the first post-reset EXIT_CANCELED from
        // the previous generation's ticker into a bogus "watchdog (CANCELED)"
        // stop — live-observed as the reboot loop dying at reboot 1/8.)
        let mut agent_service_wake = agent_console::ServiceWake::new();
        // Same probe-lifetime rule as ServiceWake above; the wake-state Arc is
        // shared with the virtio-gpu device and survives platform resets.
        let mut gpu_vblank_wake = vblank_wake::VblankWake::new();
        let gpu_vblank_wake_state = platform
            .lock()
            .expect("platform mutex for vblank wake state")
            .virtio_gpu_vblank_wake();
        // KD (kernel-debug) serial bridge, probe-lifetime like the wakers: it
        // owns the PL011 for the run when BRIDGEVM_KD_SERIAL_SOCKET is set, so
        // a WinDbg peer can attach to the guest's KDCOM stream. None otherwise,
        // leaving the serial to the boot scanner unchanged.
        let mut kd_serial_bridge = kd_serial_bridge::KdSerialBridge::from_env();
        // Keep the file offset and pending queue across guest SYSTEM_RESET.
        // Recreating the controller per boot generation replays every command
        // already consumed from an append-only control file, which can repeat
        // destructive guest actions such as a TPM-clear reboot request.
        let mut live_input = LiveInputController::from_env();

        'reboot: loop {
            // Secondary vCPUs are intentionally scoped to one boot generation in
            // Stage 1. SYSTEM_RESET joins them before resetting CPU0/platform
            // state; the next loop iteration respawns a fresh parked set.
            let drain_trace = DrainTrace {
                msix: trace_msix,
                spi: trace_spi,
            };
            let smp_trace = (smp_cpus > 1 && smp_trace_enabled).then(|| Arc::new(SmpTrace::new()));
            let pre_run_drain_gate = Arc::new(PreRunDrainGate::from_env());
            let secondary_vcpus = (smp_cpus > 1).then(|| {
                SecondaryVcpuSet::spawn(SecondaryVcpuSpawnConfig {
                    cpu_count: smp_cpus,
                    primary_vcpu: vcpu,
                    ram_base: ram as usize,
                    ram_size,
                    platform: Arc::clone(&platform),
                    drain_trace,
                    pre_run_drain_gate: Arc::clone(&pre_run_drain_gate),
                    smp_trace: smp_trace.clone(),
                })
            });
            let boot_generation = begin_watchdog_generation(&watchdog_generation);
            let watchdog_fired = Arc::new(AtomicBool::new(false));
            if watchdog_enabled {
                spawn_boot_watchdog(
                    vcpu,
                    watchdog_ms,
                    Arc::clone(&watchdog_generation),
                    boot_generation,
                    Arc::clone(&watchdog_fired),
                );
            }

            {
                let mut platform_guard = lock_platform(
                    &platform,
                    smp_trace.as_deref(),
                    0,
                    "cpu0 UART preload platform mutex",
                );
                let platform = &mut *platform_guard;
                if let Ok(input) = std::env::var("BRIDGEVM_UART_RX") {
                    platform.push_uart_input(input.as_bytes());
                    println!("UART RX preloaded: {} bytes", input.len());
                }
            }
            let mut uart_triggers = Vec::new();
            if let Some(trigger) = SerialTriggeredUartInput::from_env(
                "cd-prompt",
                "BRIDGEVM_UART_RX_ON_CD_PROMPT",
                b"Press any key to boot from CD or DVD",
            ) {
                uart_triggers.push(trigger);
            }
            if let Some(trigger) = SerialTriggeredUartInput::from_env_with_marker_env(
                "serial-marker",
                "BRIDGEVM_UART_RX_ON_SERIAL_MARKER",
                "BRIDGEVM_UART_RX_SERIAL_MARKER",
            ) {
                uart_triggers.push(trigger);
            }
            let mut xhci_hid_boot_key_triggers = Vec::new();
            if let Some(trigger) =
                XhciHidBootKeyTrigger::from_env("cd-prompt", "BRIDGEVM_XHCI_BOOT_KEY_ON_CD_PROMPT")
            {
                xhci_hid_boot_key_triggers.push(trigger);
            }
            if let Some(trigger) = XhciHidBootKeyTrigger::from_env_with_marker_env(
                "serial-marker",
                "BRIDGEVM_XHCI_BOOT_KEY_ON_SERIAL_MARKER",
                "BRIDGEVM_XHCI_BOOT_KEY_SERIAL_MARKER",
            ) {
                xhci_hid_boot_key_triggers.push(trigger);
            }
            let mut xhci_setup_input_triggers = Vec::new();
            if let Some(trigger_result) = XhciSetupInputTrigger::from_env(
                "setup-input",
                "BRIDGEVM_XHCI_SETUP_INPUT_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT_SERIAL_MARKER",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_setup_input_triggers.push(trigger),
                    Err(error) => print_setup_input_rejection("setup-input", &error),
                }
            }
            if let Some(trigger_result) = XhciSetupInputTrigger::from_env_with_timing_envs(
                "setup-input-2",
                "BRIDGEVM_XHCI_SETUP_INPUT2_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT2_SERIAL_MARKER",
                "BRIDGEVM_XHCI_SETUP_INPUT2_FIRE_DELAY_MS",
                "BRIDGEVM_XHCI_SETUP_INPUT2_RAMFB_DELAY_MS",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_setup_input_triggers.push(trigger),
                    Err(error) => print_setup_input_rejection("setup-input-2", &error),
                }
            }
            if let Some(trigger_result) = XhciSetupInputTrigger::from_env_with_timing_envs(
                "setup-input-3",
                "BRIDGEVM_XHCI_SETUP_INPUT3_ACTIONS",
                "BRIDGEVM_XHCI_SETUP_INPUT3_SERIAL_MARKER",
                "BRIDGEVM_XHCI_SETUP_INPUT3_FIRE_DELAY_MS",
                "BRIDGEVM_XHCI_SETUP_INPUT3_RAMFB_DELAY_MS",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_setup_input_triggers.push(trigger),
                    Err(error) => print_setup_input_rejection("setup-input-3", &error),
                }
            }
            let mut xhci_pointer_input_triggers = Vec::new();
            if let Some(trigger_result) = XhciPointerInputTrigger::from_env(
                "pointer-input",
                "BRIDGEVM_XHCI_POINTER_INPUT_ACTIONS",
                "BRIDGEVM_XHCI_POINTER_INPUT_SERIAL_MARKER",
            ) {
                match trigger_result {
                    Ok(trigger) => xhci_pointer_input_triggers.push(trigger),
                    Err(error) => print_pointer_input_rejection("pointer-input", &error),
                }
            }
            let mut mmio_traces: BTreeMap<&'static str, MmioTrace> = BTreeMap::new();
            let mut recent_pcie_mmio = RecentMmio::new(
                "pcie-mmio-32",
                usize::try_from(env_u64("BRIDGEVM_RECENT_PCIE_MMIO", 32)).unwrap_or(32),
            );
            let mut recent_pcie_pio = RecentMmio::new(
                "pcie-pio",
                usize::try_from(env_u64("BRIDGEVM_RECENT_PCIE_PIO", 32)).unwrap_or(32),
            );
            let mut recent_pcie_ecam = RecentPcieEcam::new(
                usize::try_from(env_u64("BRIDGEVM_RECENT_PCIE_ECAM", 128)).unwrap_or(128),
            );
            let mut recent_xhci = XhciBringupTrace::new(
                usize::try_from(env_u64("BRIDGEVM_RECENT_XHCI", 160)).unwrap_or(160),
            );
            recent_xhci.print_events_immediately(trace_xhci_bringup);
            let mut unimpl: BTreeMap<&'static str, u64> = BTreeMap::new();
            let mut redist_lo = u64::MAX;
            let mut redist_hi = 0u64;
            let mut exits = 0u64;
            let mut vtimer_exits = 0u64;
            let mut surplus_canceled_exits = 0u64;
            let mut psci_calls = 0u64;
            let mut last_pc = 0u64;
            let mut last_pre_run_pc: u64;
            let mut watch_hits = 0u32;
            let mut last_watch_pc = 0u64;
            let mut last_watch_lr = 0u64;
            let mut fwcfg_trace_count = 0u32;
            let mut drain_stats = RunLoopDrainStats::new(trace_run_loop);
            let mut ramfb_sample_loop = RamfbSampleLoop::from_env();
            let mut live_display_exporter = LiveDisplayExporter::from_env();
            let mut setup_input_host_wake = SetupInputHostWake::new();
            let boot_started = Instant::now();
            let mut boot_timer = BootTimer::from_env();
            let mut agent_console = AgentConsoleHarness::from_env(boot_started);
            let automation_always_check = !uart_triggers.is_empty()
                || !xhci_hid_boot_key_triggers.is_empty()
                || !xhci_setup_input_triggers.is_empty()
                || !xhci_pointer_input_triggers.is_empty()
                || agent_console
                    .as_ref()
                    .is_some_and(AgentConsoleHarness::per_exit_tick_needed);
            let mut automation_gate = AutomationGate::new(automation_always_check);
            // Resident service mode and measurement-safe scripted commands are
            // host-driven: without a steady waker the main loop sleeps in
            // hv_vcpu_run while the desktop idles and their tick starves (see
            // ServiceWake docs). BOOT_TIMER uses a wake no slower than 250 ms
            // and honors a shorter requested RAMFB interval. These sources
            // deliberately do not force the automation block's platform mutex
            // on every CPU0 exit. ensure_started is idempotent, so re-entering
            // here after a guest reboot is fine.
            let service_wake_interval = boot_timer
                .service_wake_interval()
                .or_else(|| {
                    agent_console
                        .as_ref()
                        .is_some_and(|harness| harness.service_wake_needed())
                        .then_some(Duration::from_millis(250))
                })
                // A guest halted at a KD breakpoint generates no vCPU exits, so
                // the pre-run drain (and the KD serial pump with it) would stall
                // until the debugger's next byte happened to arrive. A steady
                // wake keeps the debugger<->guest byte pipe flowing while halted.
                .or_else(|| {
                    kd_serial_bridge
                        .is_some()
                        .then_some(Duration::from_millis(2))
                });
            if let Some(interval) = service_wake_interval {
                agent_service_wake.ensure_started(vcpu, interval);
            }
            if let Some(state) = gpu_vblank_wake_state.as_ref() {
                gpu_vblank_wake.ensure_started(vcpu, Arc::clone(state));
            }
            let mut serial_stop_scans = SerialStopScans::default();
            let mut stop_reason;
            let mut stop_reason_code = None;
            let mut requested_system_reset = false;

            loop {
                let mut drain_pc = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut drain_pc);
                last_pre_run_pc = drain_pc;
                let pending = {
                    let mut platform_guard = lock_platform(
                        &platform,
                        smp_trace.as_deref(),
                        0,
                        "cpu0 pre-run platform mutex",
                    );
                    let platform = &mut *platform_guard;
                    if let Some(bridge) = kd_serial_bridge.as_mut() {
                        bridge.pump(platform);
                    }
                    drain_stats.prepare_pending_delivery(
                        platform,
                        &mut guest_ram,
                        drain_trace,
                        DrainContext {
                            location: DrainLocation::PreRun,
                            exit: exits,
                            pc: drain_pc,
                        },
                    )
                };
                drain_stats.complete_pending_delivery(pending, drain_trace);
                let reason = match run_hvf_vcpu_once(vcpu, exit) {
                    Ok(reason) => reason,
                    Err(r) => {
                        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                        fatal_vcpu_run_error = true;
                        stop_reason = format!("hv_vcpu_run error {r:#x}");
                        break;
                    }
                };
                exits += 1;
                if let Some(trace) = smp_trace.as_deref() {
                    trace.cpu0_progress(exits);
                }
                stop_reason_code = Some(reason);
                if let Some(action) = secondary_vcpus
                    .as_ref()
                    .and_then(SecondaryVcpuSet::terminal_action)
                {
                    psci_calls += 1;
                    match action {
                        PsciTerminalAction::SystemOff => {
                            stop_reason = format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)");
                        }
                        PsciTerminalAction::SystemReset => {
                            requested_system_reset = true;
                            stop_reason = format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)");
                        }
                    }
                    break;
                }
                let sample_tick_canceled =
                    ramfb_sample_loop.canceled_by_sample_tick(reason, &watchdog_fired);
                let setup_input_wake_canceled =
                    setup_input_host_wake.canceled_by_host_wake(reason, &watchdog_fired);
                let service_wake_canceled =
                    agent_service_wake.canceled_by_service_wake(reason, &watchdog_fired);
                let vblank_wake_canceled =
                    gpu_vblank_wake.canceled_by_vblank_wake(reason, &watchdog_fired);
                let automation_tick_canceled = sample_tick_canceled
                    || setup_input_wake_canceled
                    || service_wake_canceled
                    || vblank_wake_canceled;
                if reason == EXIT_CANCELED && !automation_tick_canceled {
                    // Two automation wakes can merge into ONE canceled exit
                    // (both hv_vcpus_exit calls land while the vCPU is still
                    // in guest mode); that single exit consumes BOTH fired
                    // flags above, and the second, sticky cancel then arrives
                    // with no flag left to claim it. Attributing such surplus
                    // cancels to the watchdog killed live boots (b2 86s,
                    // b5 258s). Only the watchdog's own flag identifies a real
                    // watchdog stop; an unclaimed cancel without it is benign.
                    if watchdog_fired.load(Ordering::SeqCst) {
                        hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                        stop_reason = "watchdog (CANCELED)".into();
                        break;
                    }
                    surplus_canceled_exits += 1;
                    continue;
                }
                if !automation_tick_canceled && reason == EXIT_VTIMER {
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    vtimer_exits += 1;
                    hv_vcpu_set_vtimer_mask(vcpu, true);
                    if exits >= MAX_EXITS {
                        stop_reason = format!("exit cap {MAX_EXITS}");
                        break;
                    }
                    continue;
                }
                if !automation_tick_canceled && reason != EXIT_EXCEPTION {
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    stop_reason = format!("exit reason {reason}");
                    break;
                }
                if !automation_tick_canceled {
                    let esr = (*exit).exception.syndrome;
                    let ec = (esr >> 26) & 0x3f;
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    match ec {
                        EC_DATA_ABORT => {
                            let ipa = (*exit).exception.physical_address;
                            let size = 1u8 << ((esr >> 22) & 0x3);
                            let srt = ((esr >> 16) & 0x1f) as u32;
                            let is_write = (esr >> 6) & 1 == 1;
                            trace_isv0_data_abort(esr, last_pc, ipa);
                            // srt=31 is WZR/XZR (stores write zero, loads
                            // discard) — never index the HV register file,
                            // where slot 31 is the PC. Linux emits `str wzr`
                            // for zero MMIO writes; this leaked the guest PC
                            // into device registers (virtio feature_select).
                            let op = if is_write {
                                let mut v = 0u64;
                                if srt != 31 {
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + srt, &mut v);
                                }
                                MmioOp::Write { size, value: v }
                            } else {
                                MmioOp::Read { size }
                            };
                            let trace_this_fwcfg = trace_fwcfg
                                && machine::FW_CFG.contains(ipa)
                                && fwcfg_trace_count < 512;
                            if trace_this_fwcfg {
                                fwcfg_trace_count += 1;
                                println!(
                                "FWCFG[{fwcfg_trace_count:03}] pc={last_pc:#x} off={:#x} op={op:?}",
                                ipa - machine::FW_CFG.base
                            );
                            }
                            let (outcome, pending, device, pcie_target, pcie_context) = {
                                let mut platform_guard = lock_platform(
                                    &platform,
                                    smp_trace.as_deref(),
                                    0,
                                    "cpu0 data-abort platform mutex",
                                );
                                let platform = &mut *platform_guard;
                                let device = machine::device_at(ipa).unwrap_or("<unmapped>");
                                let pcie_target = match device {
                                    "pcie-mmio-32" => {
                                        platform.pcie_mmio_target(ipa).map(PcieTraceTarget::Mmio)
                                    }
                                    "pcie-pio" => {
                                        platform.pcie_pio_target(ipa).map(PcieTraceTarget::Pio)
                                    }
                                    _ => None,
                                };
                                let xhci_target = match pcie_target {
                                    Some(PcieTraceTarget::Mmio(target)) => Some(target),
                                    Some(PcieTraceTarget::Pio(_)) | None => None,
                                };
                                recent_xhci.record_mmio(xhci_target, &op, &guest_ram);
                                platform.set_host_now(std::time::Instant::now());
                                let (outcome, post_drain) =
                                    platform.on_mmio_with_post_drain(ipa, op, &mut guest_ram);
                                if device == "pcie-ecam" && matches!(op, MmioOp::Write { .. }) {
                                    let base = platform.virtio_gpu_host_visible_bar_base();
                                    let mut state = hv_gpu_shm_state.lock().unwrap();
                                    state.ecam_writes = state.ecam_writes.saturating_add(1);
                                    if state.bar2_base != base {
                                        state.base_changes = state.base_changes.saturating_add(1);
                                        eprintln!(
                                            "virtio-gpu hv shm BAR2 update: ipa={ipa:#x} old={:?} new={base:?} ecam_writes={} base_changes={}",
                                            state.bar2_base,
                                            state.ecam_writes,
                                            state.base_changes
                                        );
                                    }
                                    state.bar2_base = base;
                                }
                                recent_pcie_ecam.record_after_with_context(
                                    platform,
                                    &mut guest_ram,
                                    PcieEcamAccess {
                                        pc: last_pc,
                                        ipa,
                                        exit: exits,
                                        esr,
                                        ec,
                                        srt,
                                        op: &op,
                                        outcome: &outcome,
                                        #[cfg(test)]
                                        owner_context: None,
                                    },
                                );
                                let pcie_context = targetless_xhci_trace_context(
                                    platform,
                                    &mut guest_ram,
                                    device,
                                    ipa,
                                    pcie_target,
                                    &outcome,
                                );
                                let pending = drain_stats.prepare_pending_delivery_after_mmio(
                                    platform,
                                    &mut guest_ram,
                                    drain_trace,
                                    DrainContext {
                                        location: DrainLocation::DataAbort,
                                        exit: exits,
                                        pc: last_pc,
                                    },
                                    post_drain,
                                );
                                (outcome, pending, device, pcie_target, pcie_context)
                            };
                            recent_pcie_mmio.record_with_context(PcieMmioEventInput {
                                device,
                                pc: last_pc,
                                ipa,
                                target: pcie_target,
                                op: &op,
                                outcome: &outcome,
                                context: pcie_context,
                            });
                            recent_pcie_pio.record(
                                device,
                                last_pc,
                                ipa,
                                pcie_target,
                                &op,
                                &outcome,
                            );
                            if device == "uart" && matches!(op, MmioOp::Write { .. }) {
                                automation_gate.mark_serial_output_dirty();
                            }
                            record_mmio_trace(&mut mmio_traces, device, last_pc, ipa, op, &outcome);
                            drain_stats.complete_pending_delivery(pending, drain_trace);
                            if trace_this_fwcfg {
                                println!("FWCFG[{fwcfg_trace_count:03}] -> {outcome:?}");
                            }
                            match outcome {
                                MmioOutcome::ReadValue(v) if !is_write => {
                                    if srt != 31 {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                                    }
                                }
                                MmioOutcome::ReadValue(_) | MmioOutcome::WriteAck => {}
                                MmioOutcome::KnownUnimplemented(name) => {
                                    *unimpl.entry(name).or_insert(0) += 1;
                                    if name == "gic-redist" {
                                        redist_lo = redist_lo.min(ipa);
                                        redist_hi = redist_hi.max(ipa);
                                    }
                                    if !is_write && srt != 31 {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                                    }
                                }
                                MmioOutcome::Unmapped => {
                                    *unimpl.entry("<unmapped>").or_insert(0) += 1;
                                    if !is_write && srt != 31 {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                                    }
                                }
                            }
                            hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc + 4);
                        }
                        EC_HVC => {
                            // SMCCC: PSCI (DTB method = "hvc") + ARM TRNG (RngDxe uses it).
                            let mut func = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut func);
                            match func & 0xffff_ffff {
                                SMCCC_VERSION => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0001);
                                } // SMCCC_VERSION 1.1
                                PSCI_VERSION => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0001_0001);
                                } // PSCI_VERSION 1.1
                                PSCI_FEATURES => {
                                    let mut queried = 0u64;
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut queried);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, psci_features(queried));
                                } // PSCI_FEATURES
                                PSCI_CPU_ON_32 | PSCI_CPU_ON_64 => {
                                    let mut target = 0u64;
                                    let mut entry = 0u64;
                                    let mut context = 0u64;
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 2, &mut entry);
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 3, &mut context);
                                    let result = match secondary_vcpus.as_ref() {
                                        Some(secondary_vcpus) => psci_cpu_on(
                                            &secondary_vcpus.controls,
                                            target,
                                            entry,
                                            context,
                                            smp_trace.as_deref(),
                                        ),
                                        None => PSCI_NOT_SUPPORTED,
                                    };
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
                                }
                                PSCI_CPU_OFF => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_NOT_SUPPORTED);
                                }
                                PSCI_AFFINITY_INFO_32 | PSCI_AFFINITY_INFO_64 => {
                                    let mut target = 0u64;
                                    hv_vcpu_get_reg(vcpu, HV_REG_X0 + 1, &mut target);
                                    let result = match secondary_vcpus.as_ref() {
                                        Some(secondary_vcpus) => {
                                            psci_affinity_info(&secondary_vcpus.controls, target)
                                        }
                                        None => 1,
                                    };
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, result);
                                }
                                TRNG_VERSION => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0000);
                                } // TRNG_VERSION 1.0
                                TRNG_FEATURES => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0);
                                } // TRNG_FEATURES: present
                                TRNG_GET_UUID => {
                                    // TRNG_GET_UUID
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0b0a_0908);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, 0x0f0e_0d0c);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, 0x0302_0100);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, 0x0706_0504);
                                }
                                TRNG_RND_32 | TRNG_RND_64 => {
                                    // TRNG_RND_32 / _64
                                    let r = exits
                                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                                        .wrapping_add(0xD1B5_4A32);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_SUCCESS); // SUCCESS
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, r);
                                    hv_vcpu_set_reg(
                                        vcpu,
                                        HV_REG_X0 + 2,
                                        r.rotate_left(17) ^ 0xA5A5_5A5A,
                                    );
                                    hv_vcpu_set_reg(
                                        vcpu,
                                        HV_REG_X0 + 3,
                                        r.rotate_left(41) ^ 0x1234_5678,
                                    );
                                }
                                value
                                    if psci_terminal_action(value)
                                        == Some(PsciTerminalAction::SystemOff) =>
                                {
                                    psci_calls += 1;
                                    stop_reason = format!("PSCI {PSCI_SYSTEM_OFF:#x} (system off)");
                                    break;
                                }
                                value
                                    if psci_terminal_action(value)
                                        == Some(PsciTerminalAction::SystemReset) =>
                                {
                                    psci_calls += 1;
                                    requested_system_reset = true;
                                    stop_reason =
                                        format!("PSCI {PSCI_SYSTEM_RESET:#x} (system reset)");
                                    break;
                                }
                                _ => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, PSCI_NOT_SUPPORTED);
                                } // NOT_SUPPORTED
                            }
                            // HVF reports the HVC exit PC already PAST the `hvc` instruction
                            // (unlike a data abort, where the PC is AT the faulting insn). So
                            // do NOT advance again: +4 would skip the next instruction — e.g.
                            // ArmCallHvc's `ldr x9, [sp], #0x10`, which was the RngDxe crash.
                            hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc);
                            psci_calls += 1;
                        }
                        EC_SYS_REG_TRAP => {
                            let trap = SysRegTrap::decode(esr);
                            if emulate_debug_os_lock_sysreg(vcpu, trap) {
                                hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc + 4);
                            } else {
                                stop_reason = format!(
                            "unsupported system register trap {} ESR {esr:#x} @ PC {last_pc:#x}",
                            trap.describe()
                        );
                                break;
                            }
                        }
                        EC_WATCHPOINT_LOWER | EC_WATCHPOINT_SAME => {
                            watch_hits += 1;
                            let mut lr = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_X0 + 30, &mut lr);
                            last_watch_pc = last_pc;
                            last_watch_lr = lr;
                            print!("WATCH #{watch_hits}: store @ PC {last_pc:#x} LR {lr:#x}");
                            // Single-step over the store: disable the watchpoint and arm
                            // PSTATE.SS so the store retires and we can read the new value.
                            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, 0);
                            let mut md = 0u64;
                            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut md);
                            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, md | 1); // SS
                            let mut cp = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cp);
                            hv_vcpu_set_reg(vcpu, HV_REG_CPSR, cp | (1 << 21)); // PSTATE.SS
                                                                                // do NOT advance PC: re-execute the store under single-step.
                        }
                        EC_SOFTSTEP_LOWER | EC_SOFTSTEP_SAME => {
                            let mut bytes = [0u8; 8];
                            let cur = if guest_ram.read_into(watch_target, &mut bytes) {
                                u64::from_le_bytes(bytes)
                            } else {
                                0
                            };
                            println!(" -> {watch_target:#x} = {cur:#x}");
                            // Clear single-step; re-arm the watchpoint unless we have enough.
                            let mut md = 0u64;
                            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut md);
                            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, md & !1);
                            let mut cp = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cp);
                            hv_vcpu_set_reg(vcpu, HV_REG_CPSR, cp & !(1 << 21));
                            if watch_hits < 40 {
                                hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, DBGWCR_STORE_8B);
                            }
                            // do NOT advance PC: the step already retired the instruction.
                        }
                        _ => {
                            stop_reason =
                                format!("exception EC {ec:#x} ESR {esr:#x} @ PC {last_pc:#x}");
                            break;
                        }
                    }
                }
                if exits >= MAX_EXITS {
                    stop_reason = format!("exit cap {MAX_EXITS}");
                    break;
                }
                let ramfb_checkpoint_due =
                    ramfb_sample_loop.checkpoint_due_at(std::time::Instant::now());
                let live_display_due = live_display_exporter.due(std::time::Instant::now());
                let live_input_due = live_input.poll_due(std::time::Instant::now());
                let automation_stop_reason = if automation_gate.should_check(
                    automation_tick_canceled
                        || ramfb_checkpoint_due
                        || live_display_due
                        || live_input_due,
                ) {
                    let mut platform_guard = lock_platform(
                        &platform,
                        smp_trace.as_deref(),
                        0,
                        "cpu0 automation platform mutex",
                    );
                    let platform = &mut *platform_guard;
                    automation_gate.note_checked();
                    live_input.tick(platform, &mut guest_ram, std::time::Instant::now());
                    if serial_reached_linux_panic(platform.uart_output(), &mut serial_stop_scans) {
                        Some("serial reached Linux kernel panic".into())
                    } else {
                        for trigger in &mut uart_triggers {
                            trigger.maybe_fire(platform);
                        }
                        for trigger in &mut xhci_hid_boot_key_triggers {
                            trigger.maybe_fire(platform);
                        }
                        let now = std::time::Instant::now();
                        let mut checkpoint_committed = false;
                        if let Some(agent_console) = agent_console.as_mut() {
                            agent_console.tick(platform, &mut guest_ram, now);
                            if agent_console.desktop_ready() {
                                boot_timer.observe_agent_ready(now, exits);
                                checkpoint_committed = checkpoint_glue::checkpoint_if_requested(
                                    &[vcpu],
                                    std::slice::from_raw_parts(ram, ram_size),
                                    platform,
                                )
                                .unwrap_or_else(|error| panic!("capture VM checkpoint: {error}"));
                            }
                        }
                        for trigger in &mut xhci_setup_input_triggers {
                            let now = std::time::Instant::now();
                            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                                platform,
                                &mut guest_ram,
                                now,
                                |platform, label, mem| {
                                    ramfb_dump::print_checkpoint_for_platform(label, platform, mem);
                                },
                            );
                            if let Some(deadline) =
                                trigger.pending_host_wake_deadline_at(platform, now)
                            {
                                let v = vcpu;
                                let wake_generation = Arc::clone(&watchdog_generation);
                                let wake_boot_generation = boot_generation;
                                if setup_input_host_wake.arm(deadline, move || {
                                    if watchdog_generation_matches(
                                        &wake_generation,
                                        wake_boot_generation,
                                    ) {
                                        hv_vcpus_exit(&v, 1);
                                    }
                                }) {
                                    println!(
                                        "xHCI setup-input host wake armed for delayed trigger"
                                    );
                                }
                            }
                        }
                        for trigger in &mut xhci_pointer_input_triggers {
                            let now = std::time::Instant::now();
                            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                                platform,
                                &mut guest_ram,
                                now,
                                |platform, label, mem| {
                                    ramfb_dump::print_checkpoint_for_platform(label, platform, mem);
                                },
                            );
                            if let Some(deadline) =
                                trigger.pending_host_wake_deadline_at(platform, now)
                            {
                                let v = vcpu;
                                let wake_generation = Arc::clone(&watchdog_generation);
                                let wake_boot_generation = boot_generation;
                                if setup_input_host_wake.arm(deadline, move || {
                                    if watchdog_generation_matches(
                                        &wake_generation,
                                        wake_boot_generation,
                                    ) {
                                        hv_vcpus_exit(&v, 1);
                                    }
                                }) {
                                    println!(
                                        "xHCI pointer-input host wake armed for delayed trigger"
                                    );
                                }
                            }
                        }
                        ramfb_sample_loop.emit_due(vcpu, |label| {
                            ramfb_dump::print_checkpoint_for_platform(label, platform, &guest_ram);
                        });
                        live_display_exporter.export_due(platform, std::time::Instant::now());
                        boot_timer.tick(platform, &guest_ram, exits, last_pc);
                        if checkpoint_committed {
                            Some("VM checkpoint committed; suspended process exiting".into())
                        } else if stop_on_linux
                            && serial_reached_linux_early_boot(
                                platform.uart_output(),
                                &mut serial_stop_scans,
                            )
                        {
                            Some("serial reached Linux early boot".into())
                        } else if serial_reached_shell(
                            platform.uart_output(),
                            &mut serial_stop_scans,
                        ) {
                            match ramfb_sample_loop.observe_shell(vcpu) {
                                RamfbSampleShellAction::Continue => None,
                                RamfbSampleShellAction::StopNow { reason } => Some(reason.into()),
                            }
                        } else {
                            None
                        }
                    }
                } else {
                    None
                };
                if let Some(reason) = automation_stop_reason {
                    stop_reason = reason;
                    break;
                }
            }

            // Freeze the measured duration at the VM stop boundary. Media
            // persistence and diagnostic dumps below are evidence work, not
            // guest boot time.
            let boot_timer_elapsed = boot_timer.elapsed();
            invalidate_watchdog_generation(&watchdog_generation);
            let (secondary_exit_counts, secondary_vcpu_run_error) = secondary_vcpus
                .map(SecondaryVcpuSet::shutdown_and_join)
                .unwrap_or_default();
            fatal_vcpu_run_error |= secondary_vcpu_run_error;
            if requested_system_reset {
                // Crash-survivable snapshot: capture regs + full RAM BEFORE the
                // reboot arm below wipes guest RAM / resets the vCPU. Gated on
                // BRIDGEVM_DUMP_ON_RESET; defaults to only the first reset so a
                // gen1 bugcheck (venus StartDevice) is caught, not later gens.
                if let Some(dir) = dump_on_reset_dir() {
                    if resets_dumped < dump_on_reset_max() {
                        println!(
                            "DUMP_ON_RESET: capturing reset #{resets_dumped} (reboot_count={reboot_count})"
                        );
                        dump_guest_state_on_reset(&dir, resets_dumped, vcpu, ram, ram_size);
                        resets_dumped += 1;
                    }
                }
                match decide_system_reset(reboot_count, reboot_plan) {
                    SystemResetDecision::Reboot {
                        next_reboot_count,
                        actions,
                    } => {
                        reboot_count = next_reboot_count;
                        println!(
                            "PSCI SYSTEM_RESET: reboot {reboot_count}/{}",
                            reboot_plan.max_reboots
                        );
                        let gic_reset_status = if actions.reset_gic {
                            // All secondary vCPUs have stopped and joined above, and CPU0
                            // is outside hv_vcpu_run. Apple documents hv_gic_reset as the
                            // VM-reset operation for the distributor, redistributors, and
                            // the GIC device's internal state.
                            let status = hv_gic_reset();
                            println!("hv_gic_reset = {status:#x}");
                            status
                        } else {
                            0
                        };
                        if gic_reset_status != 0 {
                            fatal_reset_error = true;
                            stop_reason = format!(
                                "hv_gic_reset failed during PSCI SYSTEM_RESET: {gic_reset_status:#x}"
                            );
                        } else {
                            if actions.reset_platform {
                                let mut platform_guard = platform.lock().expect("platform mutex");
                                let platform = &mut *platform_guard;
                                platform.reset();
                            }
                            if actions.reset_guest_ram {
                                reset_guest_ram_for_boot(&mut guest_ram, &boot_dtb);
                            }
                            if actions.reset_vcpu {
                                reset_vcpu_for_boot(vcpu);
                                arm_watchpoint_for_boot(vcpu, watch_addr);
                            }
                            if actions.continue_run_loop {
                                continue 'reboot;
                            }
                        }
                    }
                    SystemResetDecision::Stop { reason } => {
                        stop_reason = reason;
                    }
                }
            }

            let mut platform_guard = platform.lock().expect("platform mutex");
            let platform = &mut *platform_guard;
            let serial = platform.uart_output().to_vec();
            let vars_writes = media
                .flash_vars
                .persist(platform.flash_vars_image())
                .unwrap_or_else(|e| panic!("persist UEFI vars: {e}"));
            print_media_writes("UEFI vars", &vars_writes);
            if let Some(nvme) = media.nvme_disk.as_ref() {
                let writes = persist_nvme_media(platform, nvme, NvmePersistNamespace::Primary);
                print_media_writes(NvmePersistNamespace::Primary.subject(), &writes);
            }
            if let Some(target) = media.nvme_target.as_ref() {
                let writes = persist_nvme_media(platform, target, NvmePersistNamespace::Target);
                print_media_writes(NvmePersistNamespace::Target.subject(), &writes);
            }
            storage_effect_receipt::maybe_write_probe_storage_effect_receipt(
                media.nvme_disk.as_ref(),
                platform,
            );
            maybe_write_file("BRIDGEVM_BOOT_PROBE_SERIAL_OUT", &serial, "serial log");
            let symbols = symbol_lines(&serial);
            if !symbols.is_empty() {
                let text = symbols.join("\n") + "\n";
                maybe_write_file(
                    "BRIDGEVM_BOOT_PROBE_SYMBOLS_OUT",
                    text.as_bytes(),
                    "symbol log",
                );
            }

            let fp = read_reg(vcpu, HV_REG_FP);
            let lr = read_reg(vcpu, HV_REG_LR);
            let sp_el0 = read_sys_reg(vcpu, HV_SYS_REG_SP_EL0);
            let sp_el1 = read_sys_reg(vcpu, HV_SYS_REG_SP_EL1);
            let vbar_el1 = read_sys_reg(vcpu, HV_SYS_REG_VBAR_EL1);
            let elr_el1 = read_sys_reg(vcpu, HV_SYS_REG_ELR_EL1);
            let esr_el1 = read_sys_reg(vcpu, HV_SYS_REG_ESR_EL1);
            let far_el1 = read_sys_reg(vcpu, HV_SYS_REG_FAR_EL1);
            let spsr_el1 = read_sys_reg(vcpu, HV_SYS_REG_SPSR_EL1);
            let stage1_ctx = Stage1Context {
                sctlr_el1: read_sys_reg(vcpu, HV_SYS_REG_SCTLR_EL1),
                tcr_el1: read_sys_reg(vcpu, HV_SYS_REG_TCR_EL1),
                ttbr0_el1: read_sys_reg(vcpu, HV_SYS_REG_TTBR0_EL1),
                ttbr1_el1: read_sys_reg(vcpu, HV_SYS_REG_TTBR1_EL1),
                mair_el1: read_sys_reg(vcpu, HV_SYS_REG_MAIR_EL1),
            };
            println!(
                "REGS: pc={last_pc:#x} lr={lr:#x} fp={fp:#x} sp_el0={sp_el0:#x} sp_el1={sp_el1:#x}"
            );
            print_gpr_context(vcpu);
            println!(
                "STAGE1: SCTLR={:#x} MMU={} TCR={:#x} TTBR0={:#x} TTBR1={:#x} MAIR={:#x}",
                stage1_ctx.sctlr_el1,
                stage1_ctx.sctlr_el1 & 1 != 0,
                stage1_ctx.tcr_el1,
                stage1_ctx.ttbr0_el1,
                stage1_ctx.ttbr1_el1,
                stage1_ctx.mair_el1
            );
            let pc_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "pc", last_pc);
            let lr_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "lr", lr);
            let elr_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "elr", elr_el1);
            let _vbar_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "vbar", vbar_el1);
            let _far_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "far", far_el1);
            let fp_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "fp", fp);
            let _sp_el0_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "sp_el0", sp_el0);
            let sp_el1_ipa = print_stage1_translation(&guest_ram, &stage1_ctx, "sp_el1", sp_el1);
            print_pe_owner(&guest_ram, "pc", last_pc);
            print_pe_owner(&guest_ram, "lr", lr);
            print_translated_pe_owner(&guest_ram, "pc", pc_ipa);
            print_translated_pe_owner(&guest_ram, "lr", lr_ipa);
            print_translated_pe_owner(&guest_ram, "elr", elr_ipa);
            if last_watch_pc != 0 {
                print_pe_owner(&guest_ram, "watch-pc", last_watch_pc);
                print_pe_owner(&guest_ram, "watch-lr", last_watch_lr);
                let watch_pc_ipa =
                    print_stage1_translation(&guest_ram, &stage1_ctx, "watch-pc", last_watch_pc);
                let watch_lr_ipa =
                    print_stage1_translation(&guest_ram, &stage1_ctx, "watch-lr", last_watch_lr);
                print_translated_pe_owner(&guest_ram, "watch-pc", watch_pc_ipa);
                print_translated_pe_owner(&guest_ram, "watch-lr", watch_lr_ipa);
            }
            dump_guest_bytes(&guest_ram, "CODE[pc]", last_pc, 0x20, 0x60);
            dump_guest_bytes(&guest_ram, "CODE[lr]", lr, 0x28, 0x60);
            dump_translated_guest_bytes(&guest_ram, "CODE[pc]", pc_ipa, 0x20, 0x60);
            dump_translated_guest_bytes(&guest_ram, "CODE[lr]", lr_ipa, 0x28, 0x60);
            print_translated_instruction_words(&guest_ram, "CODE[pc]", last_pc, pc_ipa, 0x20, 0x60);
            print_translated_instruction_words(&guest_ram, "CODE[lr]", lr, lr_ipa, 0x28, 0x60);
            if fp != 0 {
                dump_guest_bytes(&guest_ram, "FRAME[fp]", fp, 0, 0x80);
                dump_translated_guest_bytes(&guest_ram, "FRAME[fp]", fp_ipa, 0, 0x80);
                let frame_limit =
                    usize::try_from(env_u64("BRIDGEVM_FRAME_CHAIN_LIMIT", 12)).unwrap_or(12);
                print_frame_chain(&guest_ram, &stage1_ctx, fp, frame_limit.min(64));
            }
            if sp_el1 != 0 {
                dump_guest_bytes(&guest_ram, "STACK[sp_el1]", sp_el1, 0, 0x100);
                dump_translated_guest_bytes(&guest_ram, "STACK[sp_el1]", sp_el1_ipa, 0, 0x100);
            }
            dump_env_guest_bytes(&guest_ram);

            let mut rx = [0u64; 4];
            for (i, r) in [HV_REG_X0, HV_REG_X0 + 1, HV_REG_X0 + 2, HV_REG_X0 + 9]
                .iter()
                .enumerate()
            {
                hv_vcpu_get_reg(vcpu, *r, &mut rx[i]);
            }
            println!(
                "REG-HINTS: x0={:#x} x1={:#x} x2={:#x} x9={:#x}  (x0 device: {:?})",
                rx[0],
                rx[1],
                rx[2],
                rx[3],
                machine::device_at(rx[0])
            );
            dump_guest_bytes_if_mapped(&guest_ram, "REG-HINT[x0]", rx[0], 0x40, 0x100);
            // Legacy late-DXE poll convention: x22 = polled address, x21 = expected value,
            // x20 = last read. These registers are still useful breadcrumbs, but Windows
            // high-VA/SVC stops must be read through the full GPR dump above.
            let mut ry = [0u64; 3];
            for (i, r) in [HV_REG_X0 + 20, HV_REG_X0 + 21, HV_REG_X0 + 22]
                .iter()
                .enumerate()
            {
                hv_vcpu_get_reg(vcpu, *r, &mut ry[i]);
            }
            println!(
                "LEGACY-POLL-HINT: x22(addr)={:#x} (dev {:?})  x21(expect)={:#x}  x20(last)={:#x}",
                ry[2],
                machine::device_at(ry[2]),
                ry[1],
                ry[0]
            );
            dump_guest_bytes_if_mapped(&guest_ram, "LEGACY-POLL[x22]", ry[2], 0, 0x100);
            if ry[1] != ry[2] {
                dump_guest_bytes_if_mapped(&guest_ram, "LEGACY-POLL[x21]", ry[1], 0, 0x100);
            }
            if ry[0] != ry[1] && ry[0] != ry[2] {
                dump_guest_bytes_if_mapped(&guest_ram, "LEGACY-POLL[x20]", ry[0], 0, 0x100);
            }
            let mut cpsr = 0u64;
            hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cpsr);
            println!(
                "CPSR={cpsr:#x}  DAIF: D={} A={} I(irq-masked)={} F={}  EL={}",
                (cpsr >> 9) & 1,
                (cpsr >> 8) & 1,
                (cpsr >> 7) & 1,
                (cpsr >> 6) & 1,
                (cpsr >> 2) & 3
            );
            println!(
            "EL1_EXC: VBAR={vbar_el1:#x} ELR={elr_el1:#x} ESR={esr_el1:#x} ({}) FAR={far_el1:#x} SPSR={spsr_el1:#x}",
            describe_esr(esr_el1)
        );
            print_pe_owner(&guest_ram, "elr", elr_el1);
            print_translated_pe_owner(&guest_ram, "elr", elr_ipa);
            dump_guest_bytes_if_mapped(&guest_ram, "CODE[elr]", elr_el1, 0x20, 0x60);
            dump_translated_guest_bytes(&guest_ram, "CODE[elr]", elr_ipa, 0x20, 0x60);
            print_translated_instruction_words(
                &guest_ram,
                "CODE[elr]",
                elr_el1,
                elr_ipa,
                0x20,
                0x60,
            );
            // Timer state: CTL bit0=ENABLE, bit1=IMASK, bit2=ISTATUS(fired).
            let mut tr = [0u64; 4];
            for (i, r) in [0xdf19u16, 0xdf1a, 0xdf11, 0xdf12].iter().enumerate() {
                hv_vcpu_get_sys_reg(vcpu, *r, &mut tr[i]);
            }
            println!(
                "TIMERS: CNTV_CTL={:#x} CNTV_CVAL={:#x} | CNTP_CTL={:#x} CNTP_CVAL={:#x}",
                tr[0], tr[1], tr[2], tr[3]
            );
            let mut voff = 0u64;
            hv_vcpu_get_vtimer_offset(vcpu, &mut voff);
            let mut cntvoff = 0u64;
            hv_vcpu_get_sys_reg(vcpu, 0xe703, &mut cntvoff); // CNTVOFF_EL2
            let hcnt = host_cntvct();
            let guest_cnt = hcnt.wrapping_sub(cntvoff);
            let gap = (tr[1] as i128) - (guest_cnt as i128);
            println!(
            "CNTVCT: host={hcnt:#x} CNTVOFF_EL2={cntvoff:#x} vtimer_off={voff:#x} guest~={guest_cnt:#x}  CVAL={:#x}  gap={gap} ticks (~{} s @24MHz)",
            tr[1],
            gap / 24_000_000
        );

            println!("=== EDK2 boot probe (with Apple hv_gic) ===");
            println!("stop: {stop_reason}");
            println!(
                "exits: {exits} (vtimer {vtimer_exits}, psci {psci_calls}, surplus-canceled {surplus_canceled_exits}), last PC: {last_pc:#x}"
            );
            boot_timer.print_summary(boot_timer_elapsed, exits, &secondary_exit_counts);
            drain_stats.print_summary();
            let last_prerun_pc_ipa = translated_ipa(&guest_ram, &stage1_ctx, last_pre_run_pc).ok();
            let last_nonzero_irq_drain_pc_ipa = drain_stats
                .last_nonzero_pc
                .and_then(|pc| translated_ipa(&guest_ram, &stage1_ctx, pc).ok());
            WfiWakeSummary {
                stop_reason: &stop_reason,
                stop_reason_code,
                exits,
                vtimer_exits,
                final_pc: last_pc,
                last_prerun_pc: Some(last_pre_run_pc),
                final_pc_observation: wfi_pc_observation(&guest_ram, pc_ipa),
                last_prerun_pc_observation: wfi_pc_observation(&guest_ram, last_prerun_pc_ipa),
                last_nonzero_irq_drain_pc_observation: last_nonzero_irq_drain_pc_ipa
                    .map(|ipa| wfi_pc_observation(&guest_ram, Some(ipa))),
            }
            .print(&drain_stats);
            println!("unmodelled MMIO touched: {unimpl:?}");
            print_mmio_traces(&mmio_traces);
            recent_pcie_ecam.print();
            recent_pcie_mmio.print();
            recent_pcie_pio.print();
            recent_xhci.print(platform.xhci_event_lifecycle_stats());
            print_hid_semantic_summary(platform);
            if let Some(stats) = platform.tpm_tis_stats() {
                println!(
                    "TPM2 TIS command summary: commands={} success={} errors={} backend_failures={} malformed_commands={} malformed_responses={} last_command={:#010x} clear={} startup={} self_test={} get_capability={} pcr_read={} pcr_extend={} start_auth_session={} create_primary={} read_public={} nv_read_public={} get_random={} other={}",
                    stats.commands,
                    stats.successful_responses,
                    stats.error_responses,
                    stats.backend_failures,
                    stats.malformed_commands,
                    stats.malformed_responses,
                    stats.last_command_code.unwrap_or_default(),
                    stats.clear_commands,
                    stats.startup_commands,
                    stats.self_test_commands,
                    stats.get_capability_commands,
                    stats.pcr_read_commands,
                    stats.pcr_extend_commands,
                    stats.start_auth_session_commands,
                    stats.create_primary_commands,
                    stats.read_public_commands,
                    stats.nv_read_public_commands,
                    stats.get_random_commands,
                    stats.other_commands,
                );
            }
            if let Some(stats) = platform.tpm_ppi_stats() {
                println!(
                    "TPM PPI shared-memory summary: reads={} writes={} rejected_accesses={} memory_overwrite_requested={}",
                    stats.reads,
                    stats.writes,
                    stats.rejected_accesses,
                    platform.tpm_memory_overwrite_requested(),
                );
            }
            print_nvme_command_trace(platform);
            println!("UART RX remaining bytes: {}", platform.uart_input_len());
            for trigger in &uart_triggers {
                println!(
                    "UART RX injection {}: fired={} bytes={}",
                    trigger.name(),
                    trigger.fired(),
                    trigger.bytes_len()
                );
            }
            for trigger in &xhci_hid_boot_key_triggers {
                trigger.print_summary(platform);
            }
            for trigger in &xhci_setup_input_triggers {
                trigger.print_summary(platform);
            }
            for trigger in &xhci_pointer_input_triggers {
                trigger.print_summary(platform);
            }
            if let Some(stats) = platform.pci_boot_media_stats() {
                print_block_media_stats("PCI boot-media stats", stats);
            }
            if let Some(trace) = platform.pci_boot_media_request_trace() {
                print_block_request_trace("recent PCI boot-media requests", &trace);
            }
            if let Some(stats) = platform.virtio_iso_stats() {
                print_block_media_stats("legacy virtio-mmio ISO stats", stats);
            }
            if let Some(stats) = platform.virtio_net_stats() {
                println!(
                    "virtio-net stats: notify={} tx={} rx={} tx_bytes={} rx_bytes={} status={:#x} driver_features={:#x} interrupt_status={:#x} pending_rx={}",
                    stats.notify_count,
                    stats.tx_count,
                    stats.rx_count,
                    stats.tx_bytes,
                    stats.rx_bytes,
                    stats.status,
                    stats.driver_features,
                    stats.interrupt_status,
                    stats.pending_rx_frame,
                );
                for (i, q) in stats.queues.iter().enumerate() {
                    println!(
                        "virtio-net queue[{i}]: ready={} size={} desc={:#x} last_avail_idx={} msix_vector={}",
                        q.ready, q.size, q.desc, q.last_avail_idx, q.msix_vector,
                    );
                }
            }
            if let Some(stats) = platform.virtio_net_nat_stats() {
                print_net_nat_stats(stats);
            }
            if let Some(stats) = platform.virtio_console_stats() {
                const QNAME: [&str; 6] = [
                    "port0-rx", "port0-tx", "ctrl-rx", "ctrl-tx", "port1-rx", "port1-tx",
                ];
                println!(
                    "virtio-console stats: status={:#x} driver_features={:#x} interrupt_status={:#x} \
                     port1(ready={} guest_open={} host_open={}) agent_confirmed={} \
                     pending_control={} host_to_guest_len={} host_inbound_len={}",
                    stats.status,
                    stats.driver_features,
                    stats.interrupt_status,
                    stats.port1_ready,
                    stats.port1_guest_open,
                    stats.port1_host_open,
                    stats.agent_connected_confirmed,
                    stats.pending_control,
                    stats.host_to_guest_len,
                    stats.host_inbound_len,
                );
                for (i, q) in stats.queues.iter().enumerate() {
                    // For an RX queue a healthy replenishment loop shows
                    // notify/last_avail_seen/used_produced all climbing together;
                    // a stall with last_avail_seen > last_avail_idx means the
                    // guest posted buffers we failed to consume.
                    println!(
                        "virtio-console queue[{i}] {name}: ready={ready} size={size} \
                         notify={notify} avail_seen={seen} last_consumed={consumed} \
                         used_produced={used} rx_no_buffers={nobuf} msix_vector={vec}",
                        name = QNAME[i],
                        ready = q.ready,
                        size = q.size,
                        notify = q.notify_count,
                        seen = q.last_avail_seen,
                        consumed = q.last_avail_idx,
                        used = q.used_produced,
                        nobuf = q.rx_no_buffers,
                        vec = q.msix_vector,
                    );
                }
            }
            if let Some(trace) = platform.virtio_iso_request_trace() {
                print_block_request_trace("recent legacy virtio-mmio ISO requests", &trace);
            }
            let ramfb_config = platform.ramfb_config();
            let virtio_gpu_scanout = platform.virtio_gpu_scanout();
            match ramfb_config {
                Some(config) => println!(
                    "ramfb config: addr={:#x} fourcc={:#010x} xrgb8888={} {}x{} stride={}",
                    config.addr,
                    config.fourcc,
                    config.is_xrgb8888(),
                    config.width,
                    config.height,
                    config.stride
                ),
                None => println!("ramfb config: inactive"),
            }
            ramfb_dump::print_and_dump_with_virtio_gpu(
                virtio_gpu_scanout,
                ramfb_config,
                &guest_ram,
            );
            println!("symbol lines: {}", symbols.len());
            for line in symbols.iter().rev().take(8).rev() {
                println!("{line}");
            }
            if redist_hi != 0 {
                println!(
                "gic-redist IPA range: {redist_lo:#x}..={redist_hi:#x} (redist base {:#x}, frame0 ends {:#x})",
                machine::GIC_REDIST.base,
                machine::GIC_REDIST.base + 0x20000
            );
            }
            println!("serial bytes: {}", serial.len());
            println!(
                "--- serial (tail) ---\n{}\n--- end ---",
                String::from_utf8_lossy(&serial)
            );
            break 'reboot;
        }
    }

    if fatal_vcpu_run_error || fatal_reset_error {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}
