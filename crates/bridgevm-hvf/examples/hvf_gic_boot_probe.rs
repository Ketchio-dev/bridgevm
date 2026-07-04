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

use std::alloc::{alloc_zeroed, Layout};
use std::collections::BTreeMap;
use std::os::raw::c_void;
use std::path::Path;
use std::ptr::null_mut;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::media::{
    read_bounded_file, InstallerIsoTransport, MediaWrite, MediaWriteKind, VirtBootMediaConfig,
    WritableMedia,
};
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome, VirtPlatform, VirtPlatformConfig};
use bridgevm_hvf::stage1::{self, Stage1Context, Stage1WalkStep};
use bridgevm_hvf::virtio_blk::{VirtioBlockRequestTrace, VirtioMmioBlockStats, INSTALLER_ISO_SLOT};

#[path = "hvf_gic_boot_probe/arm64_trace.rs"]
mod arm64_trace;
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
use pcie_mmio_trace::{targetless_xhci_trace_context, PcieTraceTarget, RecentMmio};
#[path = "hvf_gic_boot_probe/pcie_ecam_trace.rs"]
mod pcie_ecam_trace;
use pcie_ecam_trace::{PcieEcamAccess, PcieEcamOwnerContext, RecentPcieEcam};
#[path = "hvf_gic_boot_probe/pe_trace.rs"]
mod pe_trace;
use pe_trace::{print_frame_chain, print_pe_owner, print_translated_pe_owner, translated_ipa};
#[path = "hvf_gic_boot_probe/ramfb_dump.rs"]
mod ramfb_dump;
#[path = "hvf_gic_boot_probe/ramfb_sample_loop.rs"]
mod ramfb_sample_loop;
use ramfb_sample_loop::{RamfbSampleLoop, RamfbSampleShellAction};
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

/// A GuestMemoryMut view over the actual HVF-mapped guest RAM, so fw_cfg DMA
/// reads/writes hit real firmware memory (not a throwaway buffer).
struct MappedRam {
    base: u64,
    ptr: *mut u8,
    len: usize,
}
impl GuestMemoryMut for MappedRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())
        else {
            return false;
        };
        let Some(end) = off.checked_add(data.len()) else {
            return false;
        };
        if end > self.len {
            return false;
        }
        // SAFETY: Category 10/11 - `off..end` was checked to stay inside the
        // live HVF RAM mapping, so `ptr.add(off)` is in-bounds for `data.len()`.
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(off), data.len()) };
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa
            .checked_sub(self.base)
            .and_then(|value| usize::try_from(value).ok())?;
        let end = off.checked_add(len)?;
        if end > self.len {
            return None;
        }
        let mut v = vec![0u8; len];
        // SAFETY: Category 10/11 - `off..end` was checked to stay inside the
        // live HVF RAM mapping, so copying `len` bytes from `ptr.add(off)` is in-bounds.
        unsafe { std::ptr::copy_nonoverlapping(self.ptr.add(off), v.as_mut_ptr(), len) };
        Some(v)
    }
}

fn reset_guest_ram_for_boot(guest_ram: &mut MappedRam, dtb: &[u8]) {
    assert!(dtb.len() < guest_ram.len, "DTB must fit in guest RAM");
    unsafe {
        // SAFETY: Category 10/11 - `guest_ram.ptr` points to the live RAM mapping
        // with `guest_ram.len` bytes allocated by the probe before this helper runs.
        std::ptr::write_bytes(guest_ram.ptr, 0, guest_ram.len);
    }
    assert!(
        guest_ram.write_bytes(machine::RAM_BASE, dtb),
        "copy DTB to guest RAM base"
    );
}

#[cfg(test)]
mod mapped_ram_tests {
    use super::*;

    #[test]
    fn mapped_ram_rejects_ranges_that_overflow_host_offset() {
        let mut bytes = [0u8; 16];
        let mut ram = MappedRam {
            base: 0,
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
        let overflowing_gpa = usize::MAX as u64;

        assert_eq!(ram.read_bytes(overflowing_gpa, 2), None);
        assert!(!ram.write_bytes(overflowing_gpa, &[1, 2]));
    }

    #[test]
    fn reset_guest_ram_for_boot_zeroes_ram_and_places_dtb_at_base() {
        let mut bytes = [0xa5u8; 16];
        let mut ram = MappedRam {
            base: machine::RAM_BASE,
            ptr: bytes.as_mut_ptr(),
            len: bytes.len(),
        };
        let dtb = [1, 2, 3, 4];

        reset_guest_ram_for_boot(&mut ram, &dtb);

        assert_eq!(&bytes[..dtb.len()], dtb.as_slice());
        assert!(bytes[dtb.len()..].iter().all(|byte| *byte == 0));
    }
}

type HvReturn = i32;
type HvVcpuT = u64;
type HvGicConfig = *mut c_void;

#[repr(C)]
struct HvVcpuExitException {
    syndrome: u64,
    virtual_address: u64,
    physical_address: u64,
}
#[repr(C)]
struct HvVcpuExit {
    reason: u32,
    exception: HvVcpuExitException,
}

#[link(name = "Hypervisor", kind = "framework")]
extern "C" {
    fn hv_vm_create(config: *mut c_void) -> HvReturn;
    fn hv_vm_config_create() -> *mut c_void;
    fn hv_vm_config_set_ipa_size(config: *mut c_void, ipa_bit_length: u32) -> HvReturn;
    fn hv_vm_config_get_max_ipa_size(ipa_bit_length: *mut u32) -> HvReturn;
    fn hv_vm_config_get_el2_supported(el2_supported: *mut bool) -> HvReturn;
    fn hv_vm_config_get_el2_enabled(config: *mut c_void, el2_enabled: *mut bool) -> HvReturn;
    fn hv_vm_config_set_el2_enabled(config: *mut c_void, el2_enabled: bool) -> HvReturn;
    fn hv_vm_destroy() -> HvReturn;
    fn hv_vm_map(addr: *mut c_void, ipa: u64, size: usize, flags: u64) -> HvReturn;
    fn hv_vcpu_create(
        vcpu: *mut HvVcpuT,
        exit: *mut *mut HvVcpuExit,
        config: *mut c_void,
    ) -> HvReturn;
    fn hv_vcpu_destroy(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpu_run(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpus_exit(vcpus: *const HvVcpuT, vcpu_count: u32) -> HvReturn;
    fn hv_vcpu_get_reg(vcpu: HvVcpuT, reg: u32, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_reg(vcpu: HvVcpuT, reg: u32, value: u64) -> HvReturn;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpuT, reg: u16, value: u64) -> HvReturn;
    fn hv_vcpu_get_sys_reg(vcpu: HvVcpuT, reg: u16, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpuT, vtimer_is_masked: bool) -> HvReturn;
    fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpuT, vtimer_offset: *mut u64) -> HvReturn;
    fn hv_vcpu_set_trap_debug_exceptions(vcpu: HvVcpuT, value: bool) -> HvReturn;
    fn hv_gic_get_redistributor_base(vcpu: HvVcpuT, base: *mut u64) -> HvReturn;
    // Apple in-kernel GICv3 (macOS 15+).
    fn hv_gic_config_create() -> HvGicConfig;
    fn hv_gic_config_set_distributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_config_set_redistributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_config_set_msi_region_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_config_set_msi_interrupt_range(
        config: HvGicConfig,
        intid_base: u32,
        intid_count: u32,
    ) -> HvReturn;
    fn hv_gic_create(config: HvGicConfig) -> HvReturn;
    fn hv_gic_send_msi(address: u64, intid: u32) -> HvReturn;
    fn hv_gic_set_spi(intid: u32, level: bool) -> HvReturn;
    fn hv_gic_get_spi_interrupt_range(intid_base: *mut u32, intid_count: *mut u32) -> HvReturn;
}

const HV_REG_X0: u32 = 0;
const HV_REG_FP: u32 = 29;
const HV_REG_LR: u32 = 30;
const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const EXIT_CANCELED: u32 = 0;
const EXIT_EXCEPTION: u32 = 1;
const EXIT_VTIMER: u32 = 2;
const EC_DATA_ABORT: u64 = 0x24;
const EC_HVC: u64 = 0x16;
const EC_SYS_REG_TRAP: u64 = 0x18;
const EC_WATCHPOINT_LOWER: u64 = 0x34;
const EC_WATCHPOINT_SAME: u64 = 0x35;
const EC_SOFTSTEP_LOWER: u64 = 0x32;
const EC_SOFTSTEP_SAME: u64 = 0x33;
const HV_SYS_REG_DBGWVR0_EL1: u16 = 0x8006;
const HV_SYS_REG_DBGWCR0_EL1: u16 = 0x8007;
const HV_SYS_REG_MDSCR_EL1: u16 = 0x8012;
const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;
const HV_SYS_REG_ID_AA64DFR0_EL1: u16 = 0xc028;
const HV_SYS_REG_SCTLR_EL1: u16 = 0xc080;
const HV_SYS_REG_TTBR0_EL1: u16 = 0xc100;
const HV_SYS_REG_TTBR1_EL1: u16 = 0xc101;
const HV_SYS_REG_TCR_EL1: u16 = 0xc102;
const HV_SYS_REG_SPSR_EL1: u16 = 0xc200;
const HV_SYS_REG_ELR_EL1: u16 = 0xc201;
const HV_SYS_REG_ESR_EL1: u16 = 0xc290;
const HV_SYS_REG_FAR_EL1: u16 = 0xc300;
const HV_SYS_REG_MAIR_EL1: u16 = 0xc510;
const HV_SYS_REG_VBAR_EL1: u16 = 0xc600;
const HV_SYS_REG_SP_EL0: u16 = 0xc208;
const HV_SYS_REG_SP_EL1: u16 = 0xe208;
const HV_SYS_REG_CNTP_CTL_EL0: u16 = 0xdf11;
const HV_SYS_REG_CNTP_CVAL_EL0: u16 = 0xdf12;
const HV_SYS_REG_CNTV_CTL_EL0: u16 = 0xdf19;
const HV_SYS_REG_CNTV_CVAL_EL0: u16 = 0xdf1a;
const HV_GIC_REG_GICM_SET_SPI_NSR: u64 = 0x40;
// Watch the poll target for stores: 8-byte aligned address, BAS=0xFF (8 bytes),
// LSC=0b10 (store), PAC=0b11 (EL0+EL1), E=1. = 0x1FF7.
const WATCH_TARGET: u64 = 0x5ffd_f798;
const DBGWCR_STORE_8B: u64 = 0x1ff7;

const MAX_EXITS: u64 = 50_000_000;
const WATCHDOG_MS: u64 = 8000;
const DEFAULT_MAX_REBOOTS: u64 = 8;
const PSCI_SYSTEM_OFF: u64 = 0x8400_0008;
const PSCI_SYSTEM_RESET: u64 = 0x8400_0009;

struct HvVmGuard;

impl Drop for HvVmGuard {
    fn drop(&mut self) {
        unsafe {
            hv_vm_destroy();
        }
    }
}

struct HvVcpuGuard {
    vcpu: HvVcpuT,
}

impl Drop for HvVcpuGuard {
    fn drop(&mut self) {
        unsafe {
            hv_vcpu_destroy(self.vcpu);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SysRegTrap {
    op0: u8,
    op1: u8,
    crn: u8,
    crm: u8,
    op2: u8,
    rt: u32,
    is_read: bool,
}

impl SysRegTrap {
    fn decode(esr: u64) -> Self {
        let iss = esr & 0x01ff_ffff;
        Self {
            op0: ((iss >> 20) & 0x3) as u8,
            op2: ((iss >> 17) & 0x7) as u8,
            op1: ((iss >> 14) & 0x7) as u8,
            crn: ((iss >> 10) & 0xf) as u8,
            rt: ((iss >> 5) & 0x1f) as u32,
            crm: ((iss >> 1) & 0xf) as u8,
            is_read: (iss & 1) != 0,
        }
    }

    fn name(self) -> &'static str {
        match (self.op0, self.op1, self.crn, self.crm, self.op2) {
            (2, 0, 1, 0, 4) => "OSLAR_EL1",
            (2, 0, 1, 1, 4) => "OSLSR_EL1",
            (2, 0, 1, 3, 4) => "OSDLR_EL1",
            (3, 3, 9, 12, 0) => "PMCR_EL0",
            (3, 3, 9, 12, 1) => "PMCNTENSET_EL0",
            (3, 3, 9, 12, 2) => "PMCNTENCLR_EL0",
            (3, 3, 9, 12, 3) => "PMOVSCLR_EL0",
            (3, 0, 9, 14, 2) => "PMINTENCLR_EL1",
            (3, 3, 9, 14, 0) => "PMUSERENR_EL0",
            (3, 3, 9, 14, 1) => "PMINTENSET_EL1",
            (3, 3, 14, 15, 7) => "PMCCFILTR_EL0",
            _ => "<unknown>",
        }
    }

    fn describe(self) -> String {
        let dir = if self.is_read { "MRS" } else { "MSR" };
        format!(
            "{dir} {} (S{}_{}_C{}_C{}_{}, Rt=x{})",
            self.name(),
            self.op0,
            self.op1,
            self.crn,
            self.crm,
            self.op2,
            self.rt
        )
    }
}

fn exception_class_name(ec: u64) -> &'static str {
    match ec {
        0x00 => "unknown/uncategorized",
        0x01 => "trapped WFI/WFE",
        0x07 => "trapped FP/SIMD/SVE",
        0x15 => "SVC AArch64",
        0x16 => "HVC AArch64",
        0x18 => "trapped MSR/MRS system register",
        0x20 => "instruction abort lower EL",
        0x21 => "instruction abort same EL",
        0x24 => "data abort lower EL",
        0x25 => "data abort same EL",
        0x26 => "SP alignment fault",
        0x2f => "SError",
        0x30 => "breakpoint lower EL",
        0x31 => "breakpoint same EL",
        0x32 => "software step lower EL",
        0x33 => "software step same EL",
        0x34 => "watchpoint lower EL",
        0x35 => "watchpoint same EL",
        _ => "<unknown EC>",
    }
}

fn describe_esr(esr: u64) -> String {
    let ec = (esr >> 26) & 0x3f;
    if ec == EC_SYS_REG_TRAP {
        let trap = SysRegTrap::decode(esr);
        return format!("{}: {}", exception_class_name(ec), trap.describe());
    }
    if ec == 0x15 || ec == EC_HVC {
        let iss = esr & 0x01ff_ffff;
        let imm16 = iss & 0xffff;
        return format!(
            "{} EC={ec:#x} ISS={iss:#x} imm16={imm16:#x}",
            exception_class_name(ec)
        );
    }
    format!(
        "{} EC={ec:#x} ISS={:#x}",
        exception_class_name(ec),
        esr & 0x01ff_ffff
    )
}

#[cfg(test)]
mod esr_tests {
    use super::*;

    #[test]
    fn describes_svc_immediate() {
        assert_eq!(
            describe_esr(0x5600_1004),
            "SVC AArch64 EC=0x15 ISS=0x1004 imm16=0x1004"
        );
    }

    #[test]
    fn describes_hvc_immediate() {
        assert_eq!(
            describe_esr((EC_HVC << 26) | 0xabcd),
            "HVC AArch64 EC=0x16 ISS=0xabcd imm16=0xabcd"
        );
    }
}

fn parse_u64(s: &str) -> Option<u64> {
    let s = s.trim();
    if let Some(h) = s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        u64::from_str_radix(h, 16).ok()
    } else {
        s.parse().ok()
    }
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|s| parse_u64(&s))
        .unwrap_or(default)
}

fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

fn env_flag_default(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") | Some("on")
        | Some("ON") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("NO") => false,
        _ => default,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RebootPlan {
    max_reboots: u64,
}

impl RebootPlan {
    fn from_env() -> Self {
        Self::from_env_value(
            std::env::var("BRIDGEVM_BOOT_PROBE_MAX_REBOOTS")
                .ok()
                .as_deref(),
        )
    }

    fn from_env_value(value: Option<&str>) -> Self {
        Self {
            max_reboots: value.and_then(parse_u64).unwrap_or(DEFAULT_MAX_REBOOTS),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct RebootActions {
    reset_guest_ram: bool,
    reset_platform: bool,
    reset_vcpu: bool,
    continue_run_loop: bool,
}

impl RebootActions {
    const SYSTEM_RESET: Self = Self {
        reset_guest_ram: true,
        reset_platform: true,
        reset_vcpu: true,
        continue_run_loop: true,
    };
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum SystemResetDecision {
    Reboot {
        next_reboot_count: u64,
        actions: RebootActions,
    },
    Stop {
        reason: String,
    },
}

fn decide_system_reset(reboot_count: u64, plan: RebootPlan) -> SystemResetDecision {
    if reboot_count < plan.max_reboots {
        return SystemResetDecision::Reboot {
            next_reboot_count: reboot_count + 1,
            actions: RebootActions::SYSTEM_RESET,
        };
    }
    SystemResetDecision::Stop {
        reason: format!(
            "PSCI {PSCI_SYSTEM_RESET:#x} max reboot count {} reached",
            plan.max_reboots
        ),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PsciTerminalAction {
    SystemOff,
    SystemReset,
}

fn psci_terminal_action(func: u64) -> Option<PsciTerminalAction> {
    match func & 0xffff_ffff {
        PSCI_SYSTEM_OFF => Some(PsciTerminalAction::SystemOff),
        PSCI_SYSTEM_RESET => Some(PsciTerminalAction::SystemReset),
        _ => None,
    }
}

fn begin_watchdog_generation(generation: &AtomicU64) -> u64 {
    generation.fetch_add(1, Ordering::SeqCst).wrapping_add(1)
}

fn invalidate_watchdog_generation(generation: &AtomicU64) {
    generation.fetch_add(1, Ordering::SeqCst);
}

fn watchdog_generation_matches(generation: &AtomicU64, expected: u64) -> bool {
    generation.load(Ordering::SeqCst) == expected
}

fn spawn_boot_watchdog(
    vcpu: HvVcpuT,
    watchdog_ms: u64,
    generation: Arc<AtomicU64>,
    boot_generation: u64,
    watchdog_fired: Arc<AtomicBool>,
) {
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(watchdog_ms));
        if !watchdog_generation_matches(&generation, boot_generation) {
            return;
        }
        watchdog_fired.store(true, Ordering::SeqCst);
        let v = vcpu;
        // SAFETY: Category 8 - FFI boundary. `vcpu` is a live HVF vCPU
        // handle owned by the probe until shutdown, and `&v` points to one
        // initialized handle for the duration of this call.
        unsafe {
            hv_vcpus_exit(&v, 1);
        }
    });
}

#[cfg(test)]
mod reboot_plan_tests {
    use super::*;

    #[test]
    fn reboot_plan_resets_guest_ram_platform_and_vcpu() {
        assert_eq!(
            psci_terminal_action(PSCI_SYSTEM_OFF),
            Some(PsciTerminalAction::SystemOff)
        );
        assert_eq!(
            psci_terminal_action(PSCI_SYSTEM_RESET),
            Some(PsciTerminalAction::SystemReset)
        );
        assert_eq!(
            decide_system_reset(0, RebootPlan { max_reboots: 2 }),
            SystemResetDecision::Reboot {
                next_reboot_count: 1,
                actions: RebootActions {
                    reset_guest_ram: true,
                    reset_platform: true,
                    reset_vcpu: true,
                    continue_run_loop: true,
                },
            }
        );
    }

    #[test]
    fn reboot_guard_parses_env_and_caps_system_reset_loop() {
        assert_eq!(
            RebootPlan::from_env_value(Some("0x2")),
            RebootPlan { max_reboots: 2 }
        );
        assert_eq!(
            RebootPlan::from_env_value(Some("bad")),
            RebootPlan {
                max_reboots: DEFAULT_MAX_REBOOTS
            }
        );
        assert!(matches!(
            decide_system_reset(0, RebootPlan { max_reboots: 0 }),
            SystemResetDecision::Stop { .. }
        ));
        assert!(matches!(
            decide_system_reset(1, RebootPlan { max_reboots: 1 }),
            SystemResetDecision::Stop { .. }
        ));

        let generation = AtomicU64::new(7);
        assert!(watchdog_generation_matches(&generation, 7));
        assert!(!watchdog_generation_matches(&generation, 6));
        let boot_generation = begin_watchdog_generation(&generation);
        assert!(watchdog_generation_matches(&generation, boot_generation));
        invalidate_watchdog_generation(&generation);
        assert!(!watchdog_generation_matches(&generation, boot_generation));
    }
}

fn read_reg(vcpu: HvVcpuT, reg: u32) -> u64 {
    let mut value = 0u64;
    unsafe {
        hv_vcpu_get_reg(vcpu, reg, &mut value);
    }
    value
}

fn read_sys_reg(vcpu: HvVcpuT, reg: u16) -> u64 {
    let mut value = 0u64;
    unsafe {
        hv_vcpu_get_sys_reg(vcpu, reg, &mut value);
    }
    value
}

fn print_gpr_context(vcpu: HvVcpuT) {
    for &(start, end) in &[(0u32, 8u32), (8, 16), (16, 24), (24, 29)] {
        print!("GPRS[x{start}..x{}]:", end - 1);
        for index in start..end {
            let value = read_reg(vcpu, HV_REG_X0 + index);
            print!(" x{index}={value:#x}");
        }
        println!();
    }
}

fn reset_vcpu_for_boot(vcpu: HvVcpuT) {
    // SAFETY: Category 8 - FFI boundary. `vcpu` is the live HVF vCPU handle
    // reset on the run-loop thread while it is not inside `hv_vcpu_run`; all
    // register identifiers are HVF constants, and every output pointer below
    // is a stack local valid for the duration of its call.
    unsafe {
        for reg in HV_REG_X0..=HV_REG_LR {
            hv_vcpu_set_reg(vcpu, reg, 0);
        }
        for reg in [
            HV_SYS_REG_SCTLR_EL1,
            HV_SYS_REG_TTBR0_EL1,
            HV_SYS_REG_TTBR1_EL1,
            HV_SYS_REG_TCR_EL1,
            HV_SYS_REG_SPSR_EL1,
            HV_SYS_REG_ELR_EL1,
            HV_SYS_REG_ESR_EL1,
            HV_SYS_REG_FAR_EL1,
            HV_SYS_REG_MAIR_EL1,
            HV_SYS_REG_VBAR_EL1,
            HV_SYS_REG_SP_EL0,
            HV_SYS_REG_SP_EL1,
            HV_SYS_REG_CNTP_CTL_EL0,
            HV_SYS_REG_CNTP_CVAL_EL0,
            HV_SYS_REG_CNTV_CTL_EL0,
            HV_SYS_REG_CNTV_CVAL_EL0,
        ] {
            hv_vcpu_set_sys_reg(vcpu, reg, 0);
        }
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, 0x8000_0000);
        let mut dfr0_before = 0u64;
        let dfr0_read_status =
            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_ID_AA64DFR0_EL1, &mut dfr0_before);
        let dfr0_after = (dfr0_before & !(0xf << 8)) | (0x1 << 8);
        let dfr0_set_status = hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_ID_AA64DFR0_EL1, dfr0_after);
        println!(
            "ID_AA64DFR0_EL1 PMUVer: before={dfr0_before:#x} read={dfr0_read_status:#x} after={dfr0_after:#x} set={dfr0_set_status:#x}"
        );
        let mut rdbase = 0u64;
        let rdr = hv_gic_get_redistributor_base(vcpu, &mut rdbase);
        println!("hv_gic_get_redistributor_base(vcpu0) = {rdr:#x} -> {rdbase:#x}");
        hv_vcpu_set_reg(vcpu, HV_REG_PC, 0x0);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5);
        hv_vcpu_set_reg(vcpu, HV_REG_X0, machine::RAM_BASE);
        hv_vcpu_set_vtimer_mask(vcpu, false);
    }
}

fn arm_watchpoint_for_boot(vcpu: HvVcpuT, watch_addr: Option<u64>) {
    let Some(addr) = watch_addr else {
        return;
    };
    // SAFETY: Category 8 - FFI boundary. `vcpu` is the live HVF vCPU handle,
    // `addr` is written as a guest debug address value, and `&mut mdscr` is a
    // valid stack output pointer for the single `hv_vcpu_get_sys_reg` call.
    unsafe {
        hv_vcpu_set_trap_debug_exceptions(vcpu, true);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWVR0_EL1, addr);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, DBGWCR_STORE_8B);
        let mut mdscr = 0u64;
        hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut mdscr);
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, mdscr | (1 << 15));
    }
    println!("watchpoint armed on {addr:#x} (store)");
}

fn print_bytes(label: &str, base: u64, bytes: &[u8]) {
    print!("{label}@{base:#x}:");
    for b in bytes {
        print!("{b:02x}");
    }
    println!();
}

fn dump_guest_bytes(mem: &dyn GuestMemoryMut, label: &str, center: u64, before: u64, len: usize) {
    let Some(base) = center.checked_sub(before) else {
        println!("{label}@{center:#x}: <underflow>");
        return;
    };
    match mem.read_bytes(base, len) {
        Some(bytes) => print_bytes(label, base, &bytes),
        None => println!("{label}@{base:#x}: <not in guest RAM view>"),
    }
}

fn dump_guest_bytes_if_mapped(
    mem: &dyn GuestMemoryMut,
    label: &str,
    center: u64,
    before: u64,
    len: usize,
) {
    let Some(base) = center.checked_sub(before) else {
        return;
    };
    if let Some(bytes) = mem.read_bytes(base, len) {
        print_bytes(label, base, &bytes);
    }
}

fn dump_env_guest_bytes(mem: &dyn GuestMemoryMut) {
    let Ok(extra) = std::env::var("BRIDGEVM_BOOT_PROBE_DUMP_GPA") else {
        return;
    };
    for (idx, spec) in extra
        .split(|c: char| matches!(c, ',' | ';' | ' ' | '\n' | '\t'))
        .filter(|s| !s.trim().is_empty())
        .enumerate()
    {
        let mut parts = spec.split(':');
        let Some(gpa) = parts.next().and_then(parse_u64) else {
            println!("DUMP[env:{idx}] {spec:?}: <invalid gpa>");
            continue;
        };
        let len = parts
            .next()
            .and_then(parse_u64)
            .map(|v| v.clamp(1, 0x1000) as usize)
            .unwrap_or(0x100);
        let before = parts
            .next()
            .and_then(parse_u64)
            .map(|v| v.min(0x1000))
            .unwrap_or(0);
        dump_guest_bytes(mem, &format!("DUMP[env:{idx}]"), gpa, before, len);
    }
}

fn print_stage1_walk_steps(label: &str, steps: &[Stage1WalkStep]) {
    if !env_flag("BRIDGEVM_TRACE_STAGE1_WALKS") {
        return;
    }
    for step in steps {
        let desc = step
            .descriptor
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        let next = step
            .next_table_ipa
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        let out = step
            .output_ipa
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  WALK[{label}]: L{} table={:#x} index={:#x} entry={:#x} desc={} kind={} next={} out={}",
            step.level, step.table_ipa, step.index, step.entry_ipa, desc, step.kind, next, out
        );
    }
}

fn print_stage1_translation(
    mem: &dyn GuestMemoryMut,
    ctx: &Stage1Context,
    label: &str,
    va: u64,
) -> Option<u64> {
    if va == 0 {
        return None;
    }
    match stage1::translate(mem, ctx, va) {
        Ok(t) => {
            println!(
                "XLATE[{label}]: va={va:#x} -> ipa={:#x} root={} va_bits={} start=L{} leaf=L{}:{} desc={:#x} va_base={:#x} ipa_base={:#x} attr={} ap={} sh={} af={} pxn={} uxn={}",
                t.ipa,
                t.root.label(),
                t.va_bits,
                t.start_level,
                t.leaf_level,
                t.leaf_kind,
                t.leaf_descriptor,
                t.leaf_va_base,
                t.leaf_ipa_base,
                t.attr_index,
                t.access_permissions,
                t.shareability,
                t.access_flag,
                t.pxn,
                t.uxn
            );
            print_stage1_walk_steps(label, &t.steps);
            Some(t.ipa)
        }
        Err(failure) => {
            println!("XLATE[{label}]: va={va:#x}: {}", failure.reason);
            print_stage1_walk_steps(label, &failure.steps);
            None
        }
    }
}

fn dump_translated_guest_bytes(
    mem: &dyn GuestMemoryMut,
    label: &str,
    ipa: Option<u64>,
    before: u64,
    len: usize,
) {
    if let Some(ipa) = ipa {
        dump_guest_bytes(mem, &format!("{label}->ipa"), ipa, before, len);
    }
}

fn write_named_bytes(path: &str, bytes: &[u8], label: &str) {
    std::fs::write(path, bytes).unwrap_or_else(|e| panic!("{label} to {path}: {e}"));
    println!("{label}: {path} ({} bytes)", bytes.len());
}

fn print_media_writes(subject: &str, writes: &[MediaWrite]) {
    for write in writes {
        println!(
            "{}: {} ({} bytes)",
            write.kind.label(subject),
            write.path.display(),
            write.bytes
        );
    }
}

#[derive(Clone, Copy)]
enum NvmePersistNamespace {
    Primary,
    Target,
}

impl NvmePersistNamespace {
    fn subject(self) -> &'static str {
        match self {
            Self::Primary => "NVMe disk",
            Self::Target => "NVMe target namespace (NSID 2)",
        }
    }

    fn image_if_memory(self, platform: &VirtPlatform) -> Option<&[u8]> {
        match self {
            Self::Primary => platform.nvme_disk_if_memory(),
            Self::Target => platform.nvme_second_namespace_disk_if_memory(),
        }
    }

    fn export_snapshot(self, platform: &mut VirtPlatform, path: &Path) -> std::io::Result<u64> {
        match self {
            Self::Primary => platform.export_nvme_disk(path),
            Self::Target => platform.export_nvme_second_namespace_disk(path),
        }
    }

    fn flush(self, platform: &mut VirtPlatform) -> std::io::Result<()> {
        match self {
            Self::Primary => platform.flush_nvme_disk(),
            Self::Target => platform.flush_nvme_second_namespace_disk(),
        }
    }

    fn disk_len(self, platform: &VirtPlatform) -> u64 {
        match self {
            Self::Primary => platform.nvme_disk_len(),
            Self::Target => platform.nvme_second_namespace_disk_len().unwrap_or(0),
        }
    }
}

fn persist_nvme_media(
    platform: &mut VirtPlatform,
    media: &WritableMedia,
    namespace: NvmePersistNamespace,
) -> Vec<MediaWrite> {
    if let Some(image) = namespace.image_if_memory(platform) {
        return media
            .persist(image)
            .unwrap_or_else(|e| panic!("persist {}: {e}", namespace.subject()));
    }

    let mut writes = Vec::new();
    if let Some(path) = media.snapshot_path.as_ref() {
        let bytes = namespace
            .export_snapshot(platform, path)
            .unwrap_or_else(|e| {
                panic!(
                    "export {} snapshot {}: {e}",
                    namespace.subject(),
                    path.display()
                )
            });
        writes.push(MediaWrite {
            kind: MediaWriteKind::Snapshot,
            path: path.clone(),
            bytes: usize::try_from(bytes).unwrap_or(usize::MAX),
        });
    }
    if media.write_back {
        namespace.flush(platform).unwrap_or_else(|e| {
            panic!(
                "flush {} {}: {e}",
                namespace.subject(),
                media.path.display()
            )
        });
        writes.push(MediaWrite {
            kind: MediaWriteKind::WriteBack,
            path: media.path.clone(),
            bytes: usize::try_from(namespace.disk_len(platform)).unwrap_or(usize::MAX),
        });
    }
    writes
}

fn print_block_media_stats(label: &str, stats: VirtioMmioBlockStats) {
    println!(
        "{label}: version={} status={:#x} features={:#x} queue_ready={} queue_num={} qdesc={:#x} qavail={:#x} qused={:#x} notify={} requests={} reads={} unsupported={} io_errors={} bytes_read={} last_sector={:?} last_len={} last_status={:?}",
        stats.transport_version,
        stats.status,
        stats.driver_features,
        stats.queue_ready,
        stats.queue_num,
        stats.queue_desc,
        stats.queue_driver,
        stats.queue_device,
        stats.notify_count,
        stats.request_count,
        stats.read_count,
        stats.unsupported_count,
        stats.io_error_count,
        stats.bytes_read,
        stats.last_sector,
        stats.last_len,
        stats.last_status
    );
}

fn print_block_request_trace(label: &str, trace: &[VirtioBlockRequestTrace]) {
    println!("{label}: {} entries", trace.len());
    for entry in trace {
        println!(
            "  seq={} type={} sector={} len={} status={:#x}",
            entry.sequence, entry.request_type, entry.sector, entry.data_len, entry.status
        );
    }
}

fn maybe_write_file(path_env: &str, bytes: &[u8], description: &str) {
    if let Ok(path) = std::env::var(path_env) {
        let label = format!("{description} written");
        write_named_bytes(&path, bytes, &label);
    }
}

fn symbol_lines(serial: &[u8]) -> Vec<String> {
    String::from_utf8_lossy(serial)
        .lines()
        .filter(|line| line.starts_with("add-symbol-file "))
        .map(str::to_owned)
        .collect()
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[derive(Clone, Copy)]
enum DrainLocation {
    PreRun,
    DataAbort,
}

impl DrainLocation {
    const fn as_str(self) -> &'static str {
        match self {
            Self::PreRun => "pre-run",
            Self::DataAbort => "data-abort",
        }
    }
}

#[derive(Clone, Copy)]
struct DrainContext {
    location: DrainLocation,
    exit: u64,
    pc: u64,
}

#[derive(Clone, Copy)]
struct DrainTrace {
    msix: bool,
    spi: bool,
}

#[derive(Clone, Copy, Default)]
struct DeliveryCounts {
    drained: u64,
    success: u64,
    failure: u64,
}

impl DeliveryCounts {
    const fn has_deliveries(self) -> bool {
        self.drained != 0
    }

    fn record_status(&mut self, status: HvReturn) {
        self.drained += 1;
        if status == 0 {
            self.success += 1;
        } else {
            self.failure += 1;
        }
    }

    fn add(&mut self, other: Self) {
        self.drained += other.drained;
        self.success += other.success;
        self.failure += other.failure;
    }
}

struct RunLoopDrainStats {
    trace: bool,
    pre_run_attempts: u64,
    data_abort_attempts: u64,
    msix: DeliveryCounts,
    spi: DeliveryCounts,
    last_drain_location: Option<&'static str>,
    last_drain_exit: Option<u64>,
    last_drain_pc: Option<u64>,
    last_drain_msix: DeliveryCounts,
    last_drain_spi: DeliveryCounts,
    last_nonzero_location: Option<&'static str>,
    last_nonzero_exit: Option<u64>,
    last_nonzero_pc: Option<u64>,
}

impl RunLoopDrainStats {
    const fn new(trace: bool) -> Self {
        Self {
            trace,
            pre_run_attempts: 0,
            data_abort_attempts: 0,
            msix: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            spi: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            last_drain_location: None,
            last_drain_exit: None,
            last_drain_pc: None,
            last_drain_msix: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            last_drain_spi: DeliveryCounts {
                drained: 0,
                success: 0,
                failure: 0,
            },
            last_nonzero_location: None,
            last_nonzero_exit: None,
            last_nonzero_pc: None,
        }
    }

    fn drain_pending(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        trace: DrainTrace,
        context: DrainContext,
    ) {
        match context.location {
            DrainLocation::PreRun => self.pre_run_attempts += 1,
            DrainLocation::DataAbort => self.data_abort_attempts += 1,
        }

        // Feed host time to the platform's HID report pacing (the crate holds no
        // clock of its own). Both PreRun and DataAbort drains route through here.
        platform.set_host_now(std::time::Instant::now());
        platform.drain_xhci_setup_input_reports(mem);
        platform.drain_xhci_pointer_input_reports(mem);
        platform.poll_virtio_net(mem);
        let spi = deliver_pending_spis(platform, trace.spi);
        let msix = deliver_pending_msix(platform, trace.msix);
        self.last_drain_location = Some(context.location.as_str());
        self.last_drain_exit = Some(context.exit);
        self.last_drain_pc = Some(context.pc);
        self.last_drain_msix = msix;
        self.last_drain_spi = spi;
        self.spi.add(spi);
        self.msix.add(msix);

        if spi.has_deliveries() || msix.has_deliveries() {
            let location = context.location.as_str();
            self.last_nonzero_location = Some(location);
            self.last_nonzero_exit = Some(context.exit);
            self.last_nonzero_pc = Some(context.pc);
            if self.trace {
                println!(
                    "G004 IRQ drain: location={location} exit={} pc={:#x} msix drained={} success={} failure={} spi drained={} success={} failure={}",
                    context.exit,
                    context.pc,
                    msix.drained,
                    msix.success,
                    msix.failure,
                    spi.drained,
                    spi.success,
                    spi.failure
                );
            }
        }
    }

    fn print_summary(&self) {
        let last_drain_exit = self
            .last_drain_exit
            .map_or_else(|| "<none>".to_string(), |exit| exit.to_string());
        let last_drain_pc = self
            .last_drain_pc
            .map_or_else(|| "<none>".to_string(), |pc| format!("{pc:#x}"));
        let last_nonzero_location = self.last_nonzero_location.unwrap_or("<none>");
        println!(
            "G004 IRQ drain attempts: pre-run={} data-abort={}",
            self.pre_run_attempts, self.data_abort_attempts
        );
        println!(
            "G004 IRQ drain MSI-X: drained={} success={} failure={}",
            self.msix.drained, self.msix.success, self.msix.failure
        );
        println!(
            "G004 IRQ drain SPI: drained={} success={} failure={}",
            self.spi.drained, self.spi.success, self.spi.failure
        );
        println!(
            "G004 IRQ drain last: exit={} pc={} last_nonzero_location={}",
            last_drain_exit, last_drain_pc, last_nonzero_location
        );
    }

    fn last_drain_was_empty(&self) -> Option<bool> {
        if self.last_drain_location.is_none() {
            None
        } else {
            Some(self.last_drain_msix.drained == 0 && self.last_drain_spi.drained == 0)
        }
    }
}

const ARM64_WFI: u32 = 0xd503_207f;
const ARM64_WFE: u32 = 0xd503_205f;
const G011_INSN_WINDOW_BEFORE: u64 = 0x20;
const G011_INSN_WINDOW_LEN: usize = 0x60;

#[derive(Clone, Copy)]
struct WfiPcObservation {
    word_at: Option<u32>,
    word_before: Option<u32>,
    window_has_wfi: bool,
}

impl WfiPcObservation {
    const fn unavailable() -> Self {
        Self {
            word_at: None,
            word_before: None,
            window_has_wfi: false,
        }
    }

    fn is_wfiish(self) -> bool {
        word_is_wait_instruction(self.word_at) || word_is_wait_instruction(self.word_before)
    }
}

struct WfiWakeSummary<'a> {
    stop_reason: &'a str,
    stop_reason_code: Option<u32>,
    exits: u64,
    vtimer_exits: u64,
    final_pc: u64,
    last_prerun_pc: Option<u64>,
    final_pc_observation: WfiPcObservation,
    last_prerun_pc_observation: WfiPcObservation,
    last_nonzero_irq_drain_pc_observation: Option<WfiPcObservation>,
}

impl WfiWakeSummary<'_> {
    fn print(&self, drain_stats: &RunLoopDrainStats) {
        let last_nonzero_location = drain_stats.last_nonzero_location.unwrap_or("<none>");
        let last_nonzero_irq_drain_pc_wfiish = self
            .last_nonzero_irq_drain_pc_observation
            .map(WfiPcObservation::is_wfiish);
        println!("G011 WFI wake-source summary:");
        println!(
            "  stop={} reason_code={} exits={} watchdog_canceled={}",
            self.stop_reason,
            format_optional_u32_hex(self.stop_reason_code),
            self.exits,
            self.stop_reason_code == Some(EXIT_CANCELED)
        );
        println!(
            "  final_pc={:#x} final_pc_wfiish={} final_window_has_wfi={} final_word_at={} final_word_before={}",
            self.final_pc,
            self.final_pc_observation.is_wfiish(),
            self.final_pc_observation.window_has_wfi,
            format_optional_instruction_word(self.final_pc_observation.word_at),
            format_optional_instruction_word(self.final_pc_observation.word_before)
        );
        println!(
            "  last_prerun_pc={} last_prerun_pc_wfiish={} last_prerun_window_has_wfi={} last_prerun_word_at={} last_prerun_word_before={}",
            format_optional_u64_hex(self.last_prerun_pc),
            self.last_prerun_pc_observation.is_wfiish(),
            self.last_prerun_pc_observation.window_has_wfi,
            format_optional_instruction_word(self.last_prerun_pc_observation.word_at),
            format_optional_instruction_word(self.last_prerun_pc_observation.word_before)
        );
        println!(
            "  vtimer_exits={} msix_drained={} spi_drained={} device_event_quiescent_at_stop={}",
            self.vtimer_exits,
            drain_stats.msix.drained,
            drain_stats.spi.drained,
            format_optional_bool(drain_stats.last_drain_was_empty())
        );
        println!(
            "  last_nonzero_irq_drain=location={} exit={} pc={} pc_wfiish={}",
            last_nonzero_location,
            format_optional_u64_dec(drain_stats.last_nonzero_exit),
            format_optional_u64_hex(drain_stats.last_nonzero_pc),
            format_optional_bool(last_nonzero_irq_drain_pc_wfiish)
        );
    }
}

fn word_is_wait_instruction(word: Option<u32>) -> bool {
    matches!(word, Some(ARM64_WFI | ARM64_WFE))
}

fn read_translated_instruction_word(mem: &dyn GuestMemoryMut, ipa: Option<u64>) -> Option<u32> {
    let bytes = mem.read_bytes(ipa?, 4)?;
    let word_bytes: [u8; 4] = bytes.try_into().ok()?;
    Some(u32::from_le_bytes(word_bytes))
}

fn translated_word_before(mem: &dyn GuestMemoryMut, center_ipa: Option<u64>) -> Option<u32> {
    read_translated_instruction_word(mem, center_ipa?.checked_sub(4))
}

fn translated_window_has_wfi(mem: &dyn GuestMemoryMut, center_ipa: Option<u64>) -> bool {
    let Some(center_ipa) = center_ipa else {
        return false;
    };
    let Some(base_ipa) = center_ipa.checked_sub(G011_INSN_WINDOW_BEFORE) else {
        return false;
    };
    let Some(bytes) = mem.read_bytes(base_ipa, G011_INSN_WINDOW_LEN & !3) else {
        return false;
    };
    bytes.chunks_exact(4).any(|chunk| {
        let Ok(word_bytes) = <[u8; 4]>::try_from(chunk) else {
            return false;
        };
        u32::from_le_bytes(word_bytes) == ARM64_WFI
    })
}

fn wfi_pc_observation(mem: &dyn GuestMemoryMut, center_ipa: Option<u64>) -> WfiPcObservation {
    if center_ipa.is_none() {
        return WfiPcObservation::unavailable();
    }
    WfiPcObservation {
        word_at: read_translated_instruction_word(mem, center_ipa),
        word_before: translated_word_before(mem, center_ipa),
        window_has_wfi: translated_window_has_wfi(mem, center_ipa),
    }
}

fn format_optional_bool(value: Option<bool>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| value.to_string())
}

fn format_optional_u32_hex(value: Option<u32>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| format!("{value:#x}"))
}

fn format_optional_instruction_word(value: Option<u32>) -> String {
    value.map_or_else(
        || "<unreadable>".to_string(),
        |value| format!("{value:#010x}"),
    )
}

fn format_optional_u64_dec(value: Option<u64>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| value.to_string())
}

fn format_optional_u64_hex(value: Option<u64>) -> String {
    value.map_or_else(|| "<none>".to_string(), |value| format!("{value:#x}"))
}

#[cfg(test)]
mod wfi_summary_tests {
    use super::*;

    struct TestMem {
        base: u64,
        bytes: Vec<u8>,
    }

    impl GuestMemoryMut for TestMem {
        fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
            let Some(off) = gpa
                .checked_sub(self.base)
                .and_then(|off| usize::try_from(off).ok())
            else {
                return false;
            };
            if off + data.len() > self.bytes.len() {
                return false;
            }
            self.bytes[off..off + data.len()].copy_from_slice(data);
            true
        }

        fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
            let off = usize::try_from(gpa.checked_sub(self.base)?).ok()?;
            if off + len > self.bytes.len() {
                return None;
            }
            Some(self.bytes[off..off + len].to_vec())
        }
    }

    #[test]
    fn finds_wfi_near_translated_final_pc() {
        let center_ipa = 0x1020;
        let mut mem = TestMem {
            base: 0x1000,
            bytes: vec![0; 0x80],
        };
        assert!(mem.write_bytes(center_ipa - 4, &ARM64_WFI.to_le_bytes()));

        let observation = wfi_pc_observation(&mem, Some(center_ipa));

        assert!(observation.window_has_wfi);
        assert!(observation.is_wfiish());
        assert_eq!(observation.word_before, Some(ARM64_WFI));
    }

    #[test]
    fn reports_no_wfi_when_translation_is_missing() {
        let mem = TestMem {
            base: 0x1000,
            bytes: vec![0; 0x20],
        };

        let observation = wfi_pc_observation(&mem, Some(0x9000));

        assert!(!observation.window_has_wfi);
        assert!(!observation.is_wfiish());
        assert_eq!(observation.word_at, None);
    }
}

const XHCI_REPORT_INTERVAL_ENV: &str = "BRIDGEVM_XHCI_REPORT_INTERVAL_MS";
const XHCI_REPORT_INTERVAL_DEFAULT_MS: u64 = 30;
const XHCI_REPORT_INTERVAL_MAX_MS: u64 = 10_000;

/// Minimum host-time spacing between consecutive HID interrupt-IN reports, from
/// `BRIDGEVM_XHCI_REPORT_INTERVAL_MS` (default 30 ms; `0` disables pacing).
/// Windows drops keystrokes when a burst of reports lands microseconds apart, so
/// live runs throttle emission. Parsed leniently like the other optional
/// `BRIDGEVM_XHCI_*` knobs: a missing/invalid value falls back to the default.
fn parse_xhci_report_interval_env() -> std::time::Duration {
    let ms = match std::env::var(XHCI_REPORT_INTERVAL_ENV) {
        Ok(value) => match value.trim().parse::<u64>() {
            Ok(ms) if ms <= XHCI_REPORT_INTERVAL_MAX_MS => ms,
            Ok(ms) => {
                println!(
                    "{XHCI_REPORT_INTERVAL_ENV}={ms} exceeds max {XHCI_REPORT_INTERVAL_MAX_MS}; clamping"
                );
                XHCI_REPORT_INTERVAL_MAX_MS
            }
            Err(_) => {
                println!(
                    "{XHCI_REPORT_INTERVAL_ENV}='{}' invalid; using default {XHCI_REPORT_INTERVAL_DEFAULT_MS}",
                    value.trim()
                );
                XHCI_REPORT_INTERVAL_DEFAULT_MS
            }
        },
        Err(_) => XHCI_REPORT_INTERVAL_DEFAULT_MS,
    };
    std::time::Duration::from_millis(ms)
}

fn deliver_pending_msix(platform: &mut VirtPlatform, trace: bool) -> DeliveryCounts {
    let mut counts = DeliveryCounts::default();
    for message in platform.take_pending_msix() {
        let status = unsafe { hv_gic_send_msi(message.address, message.data) };
        counts.record_status(status);
        if trace || status != 0 {
            println!(
                "MSIX vector {} -> addr {:#x} intid {} status {status:#x}",
                message.vector, message.address, message.data
            );
        }
    }
    counts
}

fn deliver_pending_spis(platform: &mut VirtPlatform, trace: bool) -> DeliveryCounts {
    let mut counts = DeliveryCounts::default();
    for (intid, level) in platform.take_pending_spi_levels() {
        let status = unsafe { hv_gic_set_spi(intid, level) };
        counts.record_status(status);
        if trace || status != 0 {
            println!("SPI intid {intid} level={level} status {status:#x}");
        }
    }
    counts
}

fn serial_reached_shell(serial: &[u8]) -> bool {
    contains_bytes(serial, b"UEFI Interactive Shell") || contains_bytes(serial, b"Shell>")
}

fn serial_reached_linux_early_boot(serial: &[u8]) -> bool {
    contains_bytes(serial, b"Booting Linux on physical CPU")
        || contains_bytes(serial, b"Linux version")
}

fn serial_reached_linux_panic(serial: &[u8]) -> bool {
    contains_bytes(serial, b"Kernel panic")
}

fn map_file(path: &Path, ipa: u64, region_bytes: usize, flags: u64) {
    let data = read_bounded_file(path, region_bytes)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    let layout = Layout::from_size_align(region_bytes, 0x1_0000).unwrap();
    unsafe {
        let mem = alloc_zeroed(layout);
        std::ptr::copy_nonoverlapping(data.as_ptr(), mem, data.len());
        assert_eq!(
            hv_vm_map(mem as *mut c_void, ipa, region_bytes, flags),
            0,
            "map {}",
            path.display()
        );
    }
}

/// Read the host's virtual counter (Apple Silicon system counter, ~24 MHz).
fn host_cntvct() -> u64 {
    let v: u64;
    unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) v) };
    v
}

unsafe fn emulate_debug_os_lock_sysreg(vcpu: HvVcpuT, trap: SysRegTrap) -> bool {
    match (
        trap.op0,
        trap.op1,
        trap.crn,
        trap.crm,
        trap.op2,
        trap.is_read,
    ) {
        // Linux clears the Arm debug OS lock / double lock while bringing up
        // debug infrastructure. HVF traps these implementation-defined debug
        // registers; treating the writes as no-ops and reads as unlocked lets
        // the guest proceed without exposing host debug state.
        (2, 0, 1, 0, 4, false) | (2, 0, 1, 3, 4, false) => true,
        (2, 0, 1, 1, 4, true) | (2, 0, 1, 3, 4, true) => {
            if trap.rt != 31 {
                hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, 0);
            }
            true
        }
        _ => false,
    }
}

fn main() {
    let media = VirtBootMediaConfig::from_probe_env();
    let smp_cpus = env_u64("BRIDGEVM_SMP_CPUS", 1).clamp(1, machine::MAX_CPUS);
    let smp_cpus = if machine::redist_fits(smp_cpus) {
        smp_cpus
    } else {
        1
    };
    let platform_cfg = VirtPlatformConfig {
        fdt: VirtFdtConfig {
            cpu_count: smp_cpus,
            ram_size: media.ram_size,
        },
        devices: media.platform_devices,
    };
    let ram_size = usize::try_from(media.ram_size).expect("guest RAM size does not fit usize");
    assert!(
        ram_size >= 128 * 1024 * 1024,
        "guest RAM must be at least 128 MiB"
    );
    println!("Guest RAM: {} MiB", media.ram_size / (1024 * 1024));
    println!("SMP CPUs advertised: {smp_cpus}");
    let watchdog_ms = env_u64("BRIDGEVM_BOOT_PROBE_WATCHDOG_MS", WATCHDOG_MS);
    let trace_fwcfg = env_flag("BRIDGEVM_TRACE_FWCFG");
    let trace_msix = env_flag("BRIDGEVM_TRACE_MSIX");
    let trace_spi = env_flag("BRIDGEVM_TRACE_SPI");
    let trace_run_loop = env_flag("BRIDGEVM_TRACE_RUN_LOOP");
    let trace_xhci_bringup = env_flag("BRIDGEVM_TRACE_XHCI_BRINGUP");
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
        let mut platform = VirtPlatform::new_with_config(platform_cfg);
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
        reset_vcpu_for_boot(vcpu);
        arm_watchpoint_for_boot(vcpu, watch_addr);

        'reboot: loop {
            let boot_generation = begin_watchdog_generation(&watchdog_generation);
            let watchdog_fired = Arc::new(AtomicBool::new(false));
            spawn_boot_watchdog(
                vcpu,
                watchdog_ms,
                Arc::clone(&watchdog_generation),
                boot_generation,
                Arc::clone(&watchdog_fired),
            );

            if let Ok(input) = std::env::var("BRIDGEVM_UART_RX") {
                platform.push_uart_input(input.as_bytes());
                println!("UART RX preloaded: {} bytes", input.len());
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
            let mut psci_calls = 0u64;
            let mut last_pc = 0u64;
            let mut last_pre_run_pc: u64;
            let mut watch_hits = 0u32;
            let mut last_watch_pc = 0u64;
            let mut last_watch_lr = 0u64;
            let mut fwcfg_trace_count = 0u32;
            let mut drain_stats = RunLoopDrainStats::new(trace_run_loop);
            let mut ramfb_sample_loop = RamfbSampleLoop::from_env();
            let mut setup_input_host_wake = SetupInputHostWake::new();
            let drain_trace = DrainTrace {
                msix: trace_msix,
                spi: trace_spi,
            };
            let mut stop_reason;
            let mut stop_reason_code = None;
            let mut requested_system_reset = false;

            loop {
                let mut drain_pc = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut drain_pc);
                last_pre_run_pc = drain_pc;
                drain_stats.drain_pending(
                    &mut platform,
                    &mut guest_ram,
                    drain_trace,
                    DrainContext {
                        location: DrainLocation::PreRun,
                        exit: exits,
                        pc: drain_pc,
                    },
                );
                let r = hv_vcpu_run(vcpu);
                if r != 0 {
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    stop_reason = format!("hv_vcpu_run error {r:#x}");
                    break;
                }
                exits += 1;
                let reason = (*exit).reason;
                stop_reason_code = Some(reason);
                let sample_tick_canceled =
                    ramfb_sample_loop.canceled_by_sample_tick(reason, &watchdog_fired);
                let setup_input_wake_canceled =
                    setup_input_host_wake.canceled_by_host_wake(reason, &watchdog_fired);
                let automation_tick_canceled = sample_tick_canceled || setup_input_wake_canceled;
                if reason == EXIT_CANCELED && !automation_tick_canceled {
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                    stop_reason = "watchdog (CANCELED)".into();
                    break;
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
                            let op = if is_write {
                                let mut v = 0u64;
                                hv_vcpu_get_reg(vcpu, HV_REG_X0 + srt, &mut v);
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
                            let outcome = platform.on_mmio(ipa, op, &mut guest_ram);
                            let pcie_ecam_owner_context = PcieEcamOwnerContext {
                                exit: exits,
                                ipa,
                                esr,
                                ec,
                                srt,
                                serial_phase: PcieEcamOwnerContext::serial_phase_from_uart(
                                    platform.uart_output(),
                                ),
                            };
                            recent_pcie_ecam.record_after_with_context(
                                &mut platform,
                                &mut guest_ram,
                                PcieEcamAccess {
                                    pc: last_pc,
                                    ipa,
                                    op: &op,
                                    outcome: &outcome,
                                    owner_context: pcie_ecam_owner_context,
                                },
                            );
                            let pcie_context = targetless_xhci_trace_context(
                                &mut platform,
                                &mut guest_ram,
                                device,
                                ipa,
                                pcie_target,
                                &outcome,
                            );
                            recent_pcie_mmio.record_with_context(
                                device,
                                last_pc,
                                ipa,
                                pcie_target,
                                &op,
                                &outcome,
                                pcie_context,
                            );
                            recent_pcie_pio.record(
                                device,
                                last_pc,
                                ipa,
                                pcie_target,
                                &op,
                                &outcome,
                            );
                            record_mmio_trace(&mut mmio_traces, device, last_pc, ipa, op, &outcome);
                            if trace_this_fwcfg {
                                println!("FWCFG[{fwcfg_trace_count:03}] -> {outcome:?}");
                            }
                            match outcome {
                                MmioOutcome::ReadValue(v) if !is_write => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                                }
                                MmioOutcome::ReadValue(_) | MmioOutcome::WriteAck => {}
                                MmioOutcome::KnownUnimplemented(name) => {
                                    *unimpl.entry(name).or_insert(0) += 1;
                                    if name == "gic-redist" {
                                        redist_lo = redist_lo.min(ipa);
                                        redist_hi = redist_hi.max(ipa);
                                    }
                                    if !is_write {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                                    }
                                }
                                MmioOutcome::Unmapped => {
                                    *unimpl.entry("<unmapped>").or_insert(0) += 1;
                                    if !is_write {
                                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, 0);
                                    }
                                }
                            }
                            drain_stats.drain_pending(
                                &mut platform,
                                &mut guest_ram,
                                drain_trace,
                                DrainContext {
                                    location: DrainLocation::DataAbort,
                                    exit: exits,
                                    pc: last_pc,
                                },
                            );
                            hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc + 4);
                        }
                        EC_HVC => {
                            // SMCCC: PSCI (DTB method = "hvc") + ARM TRNG (RngDxe uses it).
                            let mut func = 0u64;
                            hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut func);
                            match func & 0xffff_ffff {
                                0x8000_0000 => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0001);
                                } // SMCCC_VERSION 1.1
                                0x8400_0000 => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0001_0001);
                                } // PSCI_VERSION 1.1
                                0x8400_000A => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0);
                                } // PSCI_FEATURES
                                0x8400_0050 => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0000);
                                } // TRNG_VERSION 1.0
                                0x8400_0051 => {
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0);
                                } // TRNG_FEATURES: present
                                0x8400_0052 => {
                                    // TRNG_GET_UUID
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0b0a_0908);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, 0x0f0e_0d0c);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, 0x0302_0100);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, 0x0706_0504);
                                }
                                0x8400_0053 | 0xc400_0053 => {
                                    // TRNG_RND_32 / _64
                                    let r = exits
                                        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
                                        .wrapping_add(0xD1B5_4A32);
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, 0); // SUCCESS
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
                                    hv_vcpu_set_reg(vcpu, HV_REG_X0, (-1i64) as u64);
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
                            let cur = guest_ram
                                .read_bytes(watch_target, 8)
                                .map(|b| u64::from_le_bytes(b.try_into().unwrap()))
                                .unwrap_or(0);
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
                if serial_reached_linux_panic(platform.uart_output()) {
                    stop_reason = "serial reached Linux kernel panic".into();
                    break;
                }
                for trigger in &mut uart_triggers {
                    trigger.maybe_fire(&mut platform);
                }
                for trigger in &mut xhci_hid_boot_key_triggers {
                    trigger.maybe_fire(&mut platform);
                }
                for trigger in &mut xhci_setup_input_triggers {
                    let ramfb_config = platform.ramfb_config();
                    let now = std::time::Instant::now();
                    trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                        &mut platform,
                        &mut guest_ram,
                        now,
                        |label, mem| {
                            ramfb_dump::print_checkpoint(label, ramfb_config, mem);
                        },
                    );
                    if let Some(deadline) = trigger.pending_host_wake_deadline_at(&platform, now) {
                        let v = vcpu;
                        let wake_generation = Arc::clone(&watchdog_generation);
                        let wake_boot_generation = boot_generation;
                        if setup_input_host_wake.arm(deadline, move || {
                            if watchdog_generation_matches(&wake_generation, wake_boot_generation) {
                                hv_vcpus_exit(&v, 1);
                            }
                        }) {
                            println!("xHCI setup-input host wake armed for delayed trigger");
                        }
                    }
                }
                for trigger in &mut xhci_pointer_input_triggers {
                    let ramfb_config = platform.ramfb_config();
                    let now = std::time::Instant::now();
                    trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                        &mut platform,
                        &mut guest_ram,
                        now,
                        |label, mem| {
                            ramfb_dump::print_checkpoint(label, ramfb_config, mem);
                        },
                    );
                    if let Some(deadline) = trigger.pending_host_wake_deadline_at(&platform, now) {
                        let v = vcpu;
                        let wake_generation = Arc::clone(&watchdog_generation);
                        let wake_boot_generation = boot_generation;
                        if setup_input_host_wake.arm(deadline, move || {
                            if watchdog_generation_matches(&wake_generation, wake_boot_generation) {
                                hv_vcpus_exit(&v, 1);
                            }
                        }) {
                            println!("xHCI pointer-input host wake armed for delayed trigger");
                        }
                    }
                }
                let ramfb_config = platform.ramfb_config();
                ramfb_sample_loop.emit_due(vcpu, |label| {
                    ramfb_dump::print_checkpoint(label, ramfb_config, &guest_ram);
                });
                if stop_on_linux && serial_reached_linux_early_boot(platform.uart_output()) {
                    stop_reason = "serial reached Linux early boot".into();
                    break;
                }
                if serial_reached_shell(platform.uart_output()) {
                    match ramfb_sample_loop.observe_shell(vcpu) {
                        RamfbSampleShellAction::Continue => {}
                        RamfbSampleShellAction::StopNow { reason } => {
                            stop_reason = reason.into();
                            break;
                        }
                    }
                }
            }

            invalidate_watchdog_generation(&watchdog_generation);
            if requested_system_reset {
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
                        if actions.reset_platform {
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
                    SystemResetDecision::Stop { reason } => {
                        stop_reason = reason;
                    }
                }
            }

            let serial = platform.uart_output().to_vec();
            let vars_writes = media
                .flash_vars
                .persist(platform.flash_vars_image())
                .unwrap_or_else(|e| panic!("persist UEFI vars: {e}"));
            print_media_writes("UEFI vars", &vars_writes);
            if let Some(nvme) = media.nvme_disk.as_ref() {
                let writes = persist_nvme_media(&mut platform, nvme, NvmePersistNamespace::Primary);
                print_media_writes(NvmePersistNamespace::Primary.subject(), &writes);
            }
            if let Some(target) = media.nvme_target.as_ref() {
                let writes =
                    persist_nvme_media(&mut platform, target, NvmePersistNamespace::Target);
                print_media_writes(NvmePersistNamespace::Target.subject(), &writes);
            }
            storage_effect_receipt::maybe_write_probe_storage_effect_receipt(
                media.nvme_disk.as_ref(),
                &platform,
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
                "exits: {exits} (vtimer {vtimer_exits}, psci {psci_calls}), last PC: {last_pc:#x}"
            );
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
            print_hid_semantic_summary(&platform);
            print_nvme_command_trace(&platform);
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
                trigger.print_summary(&platform);
            }
            for trigger in &xhci_setup_input_triggers {
                trigger.print_summary(&platform);
            }
            for trigger in &xhci_pointer_input_triggers {
                trigger.print_summary(&platform);
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
            if let Some(trace) = platform.virtio_iso_request_trace() {
                print_block_request_trace("recent legacy virtio-mmio ISO requests", &trace);
            }
            let ramfb_config = platform.ramfb_config();
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
            ramfb_dump::print_and_dump(ramfb_config, &guest_ram);
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
}
