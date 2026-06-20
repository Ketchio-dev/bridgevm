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
//! Optional installer ISO media (virtio-mmio block slot 31):
//!   BRIDGEVM_INSTALLER_ISO=/path/to/windows.iso ...
//!   BRIDGEVM_UART_RX=' ' ...                          # preloaded serial input bytes
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

use bridgevm_hvf::dtb::{build_virt_fdt, VirtFdtConfig};
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
use bridgevm_hvf::media::{read_bounded_file, MediaWrite, MediaWriteKind, VirtBootMediaConfig};
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome, VirtPlatform};

/// A GuestMemoryMut view over the actual HVF-mapped guest RAM, so fw_cfg DMA
/// reads/writes hit real firmware memory (not a throwaway buffer).
struct MappedRam {
    base: u64,
    ptr: *mut u8,
    len: usize,
}
impl GuestMemoryMut for MappedRam {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|o| o as usize) else {
            return false;
        };
        if off + data.len() > self.len {
            return false;
        }
        unsafe { std::ptr::copy_nonoverlapping(data.as_ptr(), self.ptr.add(off), data.len()) };
        true
    }
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa.checked_sub(self.base)? as usize;
        if off + len > self.len {
            return None;
        }
        let mut v = vec![0u8; len];
        unsafe { std::ptr::copy_nonoverlapping(self.ptr.add(off), v.as_mut_ptr(), len) };
        Some(v)
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
const HV_SYS_REG_SP_EL0: u16 = 0xc208;
const HV_SYS_REG_SP_EL1: u16 = 0xe208;
const HV_GIC_REG_GICM_SET_SPI_NSR: u64 = 0x40;
// Watch the poll target for stores: 8-byte aligned address, BAS=0xFF (8 bytes),
// LSC=0b10 (store), PAC=0b11 (EL0+EL1), E=1. = 0x1FF7.
const WATCH_TARGET: u64 = 0x5ffd_f798;
const DBGWCR_STORE_8B: u64 = 0x1ff7;

const MAX_EXITS: u64 = 50_000_000;
const WATCHDOG_MS: u64 = 8000;

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
    matches!(
        std::env::var(name).ok().as_deref(),
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
    )
}

fn env_flag_default(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref() {
        Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES") => true,
        Some("0") | Some("false") | Some("FALSE") | Some("no") | Some("NO") => false,
        _ => default,
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

#[derive(Debug, Clone)]
struct MmioTrace {
    count: u64,
    reads: u64,
    writes: u64,
    last_pc: u64,
    last_ipa: u64,
    last_op: &'static str,
    last_value: Option<u64>,
    last_outcome: &'static str,
}

impl Default for MmioTrace {
    fn default() -> Self {
        Self {
            count: 0,
            reads: 0,
            writes: 0,
            last_pc: 0,
            last_ipa: 0,
            last_op: "",
            last_value: None,
            last_outcome: "",
        }
    }
}

fn mmio_outcome_label(outcome: &MmioOutcome) -> &'static str {
    match outcome {
        MmioOutcome::ReadValue(_) => "read-value",
        MmioOutcome::WriteAck => "write-ack",
        MmioOutcome::KnownUnimplemented(_) => "known-unimplemented",
        MmioOutcome::Unmapped => "unmapped",
    }
}

fn record_mmio_trace(
    traces: &mut BTreeMap<&'static str, MmioTrace>,
    device: &'static str,
    pc: u64,
    ipa: u64,
    op: MmioOp,
    outcome: &MmioOutcome,
) {
    let entry = traces.entry(device).or_default();
    entry.count += 1;
    entry.last_pc = pc;
    entry.last_ipa = ipa;
    entry.last_outcome = mmio_outcome_label(outcome);
    match op {
        MmioOp::Read { .. } => {
            entry.reads += 1;
            entry.last_op = "read";
            entry.last_value = match outcome {
                MmioOutcome::ReadValue(value) => Some(*value),
                _ => None,
            };
        }
        MmioOp::Write { value, .. } => {
            entry.writes += 1;
            entry.last_op = "write";
            entry.last_value = Some(value);
        }
    }
}

fn print_mmio_traces(traces: &BTreeMap<&'static str, MmioTrace>) {
    println!("modelled MMIO trace:");
    for (device, trace) in traces {
        let value = trace
            .last_value
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  {device}: count={} reads={} writes={} last_pc={:#x} last_ipa={:#x} last_op={} last_value={} last_outcome={}",
            trace.count,
            trace.reads,
            trace.writes,
            trace.last_pc,
            trace.last_ipa,
            trace.last_op,
            value,
            trace.last_outcome
        );
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

fn deliver_pending_msix(platform: &mut VirtPlatform, trace: bool) {
    for message in platform.take_pending_msix() {
        let status = unsafe { hv_gic_send_msi(message.address, message.data) };
        if trace || status != 0 {
            println!(
                "MSIX vector {} -> addr {:#x} intid {} status {status:#x}",
                message.vector, message.address, message.data
            );
        }
    }
}

fn deliver_pending_spis(platform: &mut VirtPlatform, trace: bool) {
    for (intid, level) in platform.take_pending_spi_levels() {
        let status = unsafe { hv_gic_set_spi(intid, level) };
        if trace || status != 0 {
            println!("SPI intid {intid} level={level} status {status:#x}");
        }
    }
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
    let ram_size = usize::try_from(media.ram_size).expect("guest RAM size does not fit usize");
    assert!(
        ram_size >= 128 * 1024 * 1024,
        "guest RAM must be at least 128 MiB"
    );
    println!("Guest RAM: {} MiB", media.ram_size / (1024 * 1024));
    let watchdog_ms = env_u64("BRIDGEVM_BOOT_PROBE_WATCHDOG_MS", WATCHDOG_MS);
    let trace_fwcfg = env_flag("BRIDGEVM_TRACE_FWCFG");
    let trace_msix = env_flag("BRIDGEVM_TRACE_MSIX");
    let trace_spi = env_flag("BRIDGEVM_TRACE_SPI");
    let stop_on_linux = env_flag_default("BRIDGEVM_BOOT_PROBE_STOP_ON_LINUX", true);

    unsafe {
        // Create the VM with the max IPA size: the PCIe ECAM sits at 256 GiB,
        // beyond the 36-bit default IPA window.
        let vmcfg = hv_vm_config_create();
        let mut max_ipa = 0u32;
        hv_vm_config_get_max_ipa_size(&mut max_ipa);
        hv_vm_config_set_ipa_size(vmcfg, max_ipa);
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

        let ram_layout = Layout::from_size_align(ram_size, 0x1_0000).unwrap();
        let ram = alloc_zeroed(ram_layout);
        let dtb = build_virt_fdt(&VirtFdtConfig {
            cpu_count: 1,
            ram_size: media.ram_size,
        });
        assert!(dtb.len() < ram_size, "DTB must fit in guest RAM");
        std::ptr::copy_nonoverlapping(dtb.as_ptr(), ram, dtb.len());
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
        // MPIDR_EL1 affinity 0 (bit 31 RES1) — Apple hv_gic associates this vCPU's
        // redistributor frame from its MPIDR, so this must be set before the GIC
        // redistributor MMIO is served or hv_gic_get_redistributor_base is called.
        const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, 0x8000_0000);
        let mut rdbase = 0u64;
        let rdr = hv_gic_get_redistributor_base(vcpu, &mut rdbase);
        println!("hv_gic_get_redistributor_base(vcpu0) = {rdr:#x} -> {rdbase:#x}");
        hv_vcpu_set_reg(vcpu, HV_REG_PC, 0x0);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5);
        hv_vcpu_set_reg(vcpu, HV_REG_X0, machine::RAM_BASE);
        // Unmask the virtual timer at the HVF level so it can fire (and hv_gic can
        // route the timer PPI). It is masked by default, which is why no
        // VTIMER_ACTIVATED ever arrived and the firmware spun on its ISR flag.
        hv_vcpu_set_vtimer_mask(vcpu, false);

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
        if let Some(addr) = watch_addr {
            hv_vcpu_set_trap_debug_exceptions(vcpu, true);
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWVR0_EL1, addr);
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_DBGWCR0_EL1, DBGWCR_STORE_8B);
            let mut mdscr = 0u64;
            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, &mut mdscr);
            hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MDSCR_EL1, mdscr | (1 << 15)); // MDE
            println!("watchpoint armed on {addr:#x} (store)");
        }

        let vcpu_for_wd = vcpu;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(watchdog_ms));
            let v = vcpu_for_wd;
            hv_vcpus_exit(&v, 1);
        });

        let mut platform = VirtPlatform::new(VirtFdtConfig {
            cpu_count: 1,
            ram_size: media.ram_size,
        });
        if let Ok(input) = std::env::var("BRIDGEVM_UART_RX") {
            platform.push_uart_input(input.as_bytes());
            println!("UART RX preloaded: {} bytes", input.len());
        }
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
        if let Some(path) = media.installer_iso_path.as_ref() {
            platform
                .attach_virtio_iso(path)
                .unwrap_or_else(|e| panic!("attach installer ISO {}: {e}", path.display()));
            println!(
                "Installer ISO attached on virtio-mmio slot {}: {}",
                bridgevm_hvf::virtio_blk::INSTALLER_ISO_SLOT,
                path.display()
            );
        }
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
        let mut mmio_traces: BTreeMap<&'static str, MmioTrace> = BTreeMap::new();
        let mut unimpl: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut redist_lo = u64::MAX;
        let mut redist_hi = 0u64;
        let mut exits = 0u64;
        let mut vtimer_exits = 0u64;
        let mut psci_calls = 0u64;
        let mut last_pc = 0u64;
        let mut watch_hits = 0u32;
        let mut fwcfg_trace_count = 0u32;
        let stop_reason;

        loop {
            let r = hv_vcpu_run(vcpu);
            if r != 0 {
                stop_reason = format!("hv_vcpu_run error {r:#x}");
                break;
            }
            exits += 1;
            let reason = (*exit).reason;
            if reason == EXIT_CANCELED {
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                stop_reason = "watchdog (CANCELED)".into();
                break;
            }
            if reason == EXIT_VTIMER {
                vtimer_exits += 1;
                hv_vcpu_set_vtimer_mask(vcpu, true);
                if exits >= MAX_EXITS {
                    stop_reason = format!("exit cap {MAX_EXITS}");
                    break;
                }
                continue;
            }
            if reason != EXIT_EXCEPTION {
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut last_pc);
                stop_reason = format!("exit reason {reason}");
                break;
            }
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
                    let trace_this_fwcfg =
                        trace_fwcfg && machine::FW_CFG.contains(ipa) && fwcfg_trace_count < 512;
                    if trace_this_fwcfg {
                        fwcfg_trace_count += 1;
                        println!(
                            "FWCFG[{fwcfg_trace_count:03}] pc={last_pc:#x} off={:#x} op={op:?}",
                            ipa - machine::FW_CFG.base
                        );
                    }
                    let outcome = platform.on_mmio(ipa, op, &mut guest_ram);
                    let device = machine::device_at(ipa).unwrap_or("<unmapped>");
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
                    deliver_pending_spis(&mut platform, trace_spi);
                    deliver_pending_msix(&mut platform, trace_msix);
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
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, r.rotate_left(17) ^ 0xA5A5_5A5A);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, r.rotate_left(41) ^ 0x1234_5678);
                        }
                        0x8400_0008 | 0x8400_0009 => {
                            stop_reason =
                                format!("PSCI {:#x} (system off/reset)", func & 0xffff_ffff);
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
                    stop_reason = format!("exception EC {ec:#x} ESR {esr:#x} @ PC {last_pc:#x}");
                    break;
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
            if stop_on_linux && serial_reached_linux_early_boot(platform.uart_output()) {
                stop_reason = "serial reached Linux early boot".into();
                break;
            }
            if serial_reached_shell(platform.uart_output()) {
                stop_reason = "serial reached UEFI shell".into();
                break;
            }
        }

        let serial = platform.uart_output().to_vec();
        let vars_writes = media
            .flash_vars
            .persist(platform.flash_vars_image())
            .unwrap_or_else(|e| panic!("persist UEFI vars: {e}"));
        print_media_writes("UEFI vars", &vars_writes);
        if let Some(nvme) = media.nvme_disk.as_ref() {
            let writes = if let Some(image) = platform.nvme_disk_if_memory() {
                nvme.persist(image)
                    .unwrap_or_else(|e| panic!("persist NVMe disk: {e}"))
            } else {
                let mut writes = Vec::new();
                if let Some(path) = nvme.snapshot_path.as_ref() {
                    let bytes = platform.export_nvme_disk(path).unwrap_or_else(|e| {
                        panic!("export NVMe disk snapshot {}: {e}", path.display())
                    });
                    writes.push(MediaWrite {
                        kind: MediaWriteKind::Snapshot,
                        path: path.clone(),
                        bytes: usize::try_from(bytes).unwrap_or(usize::MAX),
                    });
                }
                if nvme.write_back {
                    platform
                        .flush_nvme_disk()
                        .unwrap_or_else(|e| panic!("flush NVMe disk {}: {e}", nvme.path.display()));
                    writes.push(MediaWrite {
                        kind: MediaWriteKind::WriteBack,
                        path: nvme.path.clone(),
                        bytes: usize::try_from(platform.nvme_disk_len()).unwrap_or(usize::MAX),
                    });
                }
                writes
            };
            print_media_writes("NVMe disk", &writes);
        }
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
        println!(
            "REGS: pc={last_pc:#x} lr={lr:#x} fp={fp:#x} sp_el0={sp_el0:#x} sp_el1={sp_el1:#x}"
        );
        dump_guest_bytes(&guest_ram, "CODE[pc]", last_pc, 0x20, 0x60);
        dump_guest_bytes(&guest_ram, "CODE[lr]", lr, 0x28, 0x60);
        if fp != 0 {
            dump_guest_bytes(&guest_ram, "FRAME[fp]", fp, 0, 0x80);
        }
        if sp_el1 != 0 {
            dump_guest_bytes(&guest_ram, "STACK[sp_el1]", sp_el1, 0, 0x100);
        }
        dump_env_guest_bytes(&guest_ram);

        // What is the firmware polling at the stop point? x0 is MmioRead32's address arg.
        let mut rx = [0u64; 4];
        for (i, r) in [HV_REG_X0, HV_REG_X0 + 1, HV_REG_X0 + 2, HV_REG_X0 + 9]
            .iter()
            .enumerate()
        {
            hv_vcpu_get_reg(vcpu, *r, &mut rx[i]);
        }
        println!(
            "AT-STOP: x0={:#x} x1={:#x} x2={:#x} x9={:#x}  (x0 device: {:?})",
            rx[0],
            rx[1],
            rx[2],
            rx[3],
            machine::device_at(rx[0])
        );
        dump_guest_bytes_if_mapped(&guest_ram, "AT-STOP[x0]", rx[0], 0x40, 0x100);
        // Poll-loop state: x22 = polled address, x21 = expected value, x20 = last read.
        let mut ry = [0u64; 3];
        for (i, r) in [HV_REG_X0 + 20, HV_REG_X0 + 21, HV_REG_X0 + 22]
            .iter()
            .enumerate()
        {
            hv_vcpu_get_reg(vcpu, *r, &mut ry[i]);
        }
        println!(
            "POLL: x22(addr)={:#x} (dev {:?})  x21(expect)={:#x}  x20(last)={:#x}",
            ry[2],
            machine::device_at(ry[2]),
            ry[1],
            ry[0]
        );
        dump_guest_bytes_if_mapped(&guest_ram, "POLL[x22]", ry[2], 0, 0x100);
        if ry[1] != ry[2] {
            dump_guest_bytes_if_mapped(&guest_ram, "POLL[x21]", ry[1], 0, 0x100);
        }
        if ry[0] != ry[1] && ry[0] != ry[2] {
            dump_guest_bytes_if_mapped(&guest_ram, "POLL[x20]", ry[0], 0, 0x100);
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
        println!("unmodelled MMIO touched: {unimpl:?}");
        print_mmio_traces(&mmio_traces);
        println!("UART RX remaining bytes: {}", platform.uart_input_len());
        if let Some(stats) = platform.virtio_iso_stats() {
            println!(
                "virtio ISO stats: version={} status={:#x} features={:#x} queue_ready={} queue_num={} qdesc={:#x} qavail={:#x} qused={:#x} notify={} requests={} reads={} unsupported={} io_errors={} bytes_read={} last_sector={:?} last_len={} last_status={:?}",
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
    }
}
