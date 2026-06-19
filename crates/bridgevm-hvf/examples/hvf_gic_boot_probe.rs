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

use std::alloc::{alloc_zeroed, Layout};
use std::collections::BTreeMap;
use std::os::raw::c_void;
use std::ptr::null_mut;

use bridgevm_hvf::dtb::{build_virt_fdt, VirtFdtConfig};
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::machine;
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
    fn hv_vcpu_create(vcpu: *mut HvVcpuT, exit: *mut *mut HvVcpuExit, config: *mut c_void) -> HvReturn;
    fn hv_vcpu_destroy(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpu_run(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpus_exit(vcpus: *const HvVcpuT, vcpu_count: u32) -> HvReturn;
    fn hv_vcpu_get_reg(vcpu: HvVcpuT, reg: u32, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_reg(vcpu: HvVcpuT, reg: u32, value: u64) -> HvReturn;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpuT, reg: u16, value: u64) -> HvReturn;
    fn hv_vcpu_get_sys_reg(vcpu: HvVcpuT, reg: u16, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpuT, vtimer_is_masked: bool) -> HvReturn;
    fn hv_gic_get_redistributor_base(vcpu: HvVcpuT, base: *mut u64) -> HvReturn;
    // Apple in-kernel GICv3 (macOS 15+).
    fn hv_gic_config_create() -> HvGicConfig;
    fn hv_gic_config_set_distributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_config_set_redistributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_create(config: HvGicConfig) -> HvReturn;
}

const HV_REG_X0: u32 = 0;
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

const RAM_SIZE: usize = 0x2000_0000; // 512 MiB
const MAX_EXITS: u64 = 50_000_000;
const WATCHDOG_MS: u64 = 8000;

fn map_file(path: &str, ipa: u64, region_bytes: usize, flags: u64) {
    let data = std::fs::read(path).unwrap_or_else(|e| panic!("read {path}: {e}"));
    assert!(data.len() <= region_bytes, "{path} larger than region");
    let layout = Layout::from_size_align(region_bytes, 0x1_0000).unwrap();
    unsafe {
        let mem = alloc_zeroed(layout);
        std::ptr::copy_nonoverlapping(data.as_ptr(), mem, data.len());
        assert_eq!(hv_vm_map(mem as *mut c_void, ipa, region_bytes, flags), 0, "map {path}");
    }
}

fn main() {
    let code = std::env::var("BRIDGEVM_AARCH64_UEFI_CODE")
        .unwrap_or_else(|_| "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd".into());
    let vars = std::env::var("BRIDGEVM_AARCH64_UEFI_VARS")
        .unwrap_or_else(|_| "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd".into());

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

        // In-kernel GICv3 must be created after the VM and before any vCPU.
        let gic = hv_gic_config_create();
        assert_eq!(hv_gic_config_set_distributor_base(gic, machine::GIC_DIST.base), 0, "set dist base");
        assert_eq!(hv_gic_config_set_redistributor_base(gic, machine::GIC_REDIST.base), 0, "set redist base");
        let gic_r = hv_gic_create(gic);
        println!("hv_gic_create = {gic_r:#x} (dist {:#x}, redist {:#x})", machine::GIC_DIST.base, machine::GIC_REDIST.base);
        assert_eq!(gic_r, 0, "hv_gic_create");

        map_file(&code, machine::FLASH_CODE.base, machine::FLASH_CODE.size as usize, HV_MEMORY_READ | HV_MEMORY_EXEC);
        map_file(&vars, machine::FLASH_VARS.base, machine::FLASH_VARS.size as usize, HV_MEMORY_READ | HV_MEMORY_WRITE);

        let ram_layout = Layout::from_size_align(RAM_SIZE, 0x1_0000).unwrap();
        let ram = alloc_zeroed(ram_layout);
        let dtb = build_virt_fdt(&VirtFdtConfig { cpu_count: 1, ram_size: RAM_SIZE as u64 });
        std::ptr::copy_nonoverlapping(dtb.as_ptr(), ram, dtb.len());
        assert_eq!(
            hv_vm_map(ram as *mut c_void, machine::RAM_BASE, RAM_SIZE, HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC),
            0,
            "map ram"
        );

        let mut vcpu: HvVcpuT = 0;
        let mut exit: *mut HvVcpuExit = null_mut();
        assert_eq!(hv_vcpu_create(&mut vcpu, &mut exit, null_mut()), 0, "hv_vcpu_create");
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

        let vcpu_for_wd = vcpu;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(WATCHDOG_MS));
            let v = vcpu_for_wd;
            hv_vcpus_exit(&v, 1);
        });

        let mut platform = VirtPlatform::new(VirtFdtConfig { cpu_count: 1, ram_size: RAM_SIZE as u64 });
        let mut guest_ram = MappedRam { base: machine::RAM_BASE, ptr: ram, len: RAM_SIZE };
        let mut unimpl: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut redist_lo = u64::MAX;
        let mut redist_hi = 0u64;
        let mut exits = 0u64;
        let mut vtimer_exits = 0u64;
        let mut psci_calls = 0u64;
        let mut last_pc = 0u64;
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
                    match platform.on_mmio(ipa, op, &mut guest_ram) {
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
                    hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc + 4);
                }
                EC_HVC => {
                    // SMCCC: PSCI (DTB method = "hvc") + ARM TRNG (RngDxe uses it).
                    let mut func = 0u64;
                    hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut func);
                    match func & 0xffff_ffff {
                        0x8000_0000 => { hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0001); }    // SMCCC_VERSION 1.1
                        0x8400_0000 => { hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0001_0001); } // PSCI_VERSION 1.1
                        0x8400_000A => { hv_vcpu_set_reg(vcpu, HV_REG_X0, 0); }           // PSCI_FEATURES
                        0x8400_0050 => { hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x1_0000); }    // TRNG_VERSION 1.0
                        0x8400_0051 => { hv_vcpu_set_reg(vcpu, HV_REG_X0, 0); }           // TRNG_FEATURES: present
                        0x8400_0052 => {                                                  // TRNG_GET_UUID
                            hv_vcpu_set_reg(vcpu, HV_REG_X0, 0x0b0a_0908);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, 0x0f0e_0d0c);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, 0x0302_0100);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, 0x0706_0504);
                        }
                        0x8400_0053 | 0xc400_0053 => {                                    // TRNG_RND_32 / _64
                            let r = exits.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(0xD1B5_4A32);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0, 0);                          // SUCCESS
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 1, r);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 2, r.rotate_left(17) ^ 0xA5A5_5A5A);
                            hv_vcpu_set_reg(vcpu, HV_REG_X0 + 3, r.rotate_left(41) ^ 0x1234_5678);
                        }
                        0x8400_0008 | 0x8400_0009 => {
                            stop_reason = format!("PSCI {:#x} (system off/reset)", func & 0xffff_ffff);
                            break;
                        }
                        _ => { hv_vcpu_set_reg(vcpu, HV_REG_X0, (-1i64) as u64); }         // NOT_SUPPORTED
                    }
                    // HVF reports the HVC exit PC already PAST the `hvc` instruction
                    // (unlike a data abort, where the PC is AT the faulting insn). So
                    // do NOT advance again: +4 would skip the next instruction — e.g.
                    // ArmCallHvc's `ldr x9, [sp], #0x10`, which was the RngDxe crash.
                    hv_vcpu_set_reg(vcpu, HV_REG_PC, last_pc);
                    psci_calls += 1;
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
        }

        let serial = platform.uart_output().to_vec();

        // Dump the code around the current frontier PC (read from guest RAM) so it
        // can be disassembled directly. Set to wherever the firmware currently
        // stops (`last PC` in the summary).
        let crash_pc = 0x5fcf_13b0u64;
        if let Some(code) = guest_ram.read_bytes(crash_pc - 0x18, 0x48) {
            print!("CODE@{:#x}:", crash_pc - 0x18);
            for b in &code {
                print!("{:02x}", b);
            }
            println!();
        }

        // What is the firmware polling at the stop point? x0 is MmioRead32's address arg.
        let mut rx = [0u64; 4];
        for (i, r) in [HV_REG_X0, HV_REG_X0 + 1, HV_REG_X0 + 2, HV_REG_X0 + 9].iter().enumerate() {
            hv_vcpu_get_reg(vcpu, *r, &mut rx[i]);
        }
        println!(
            "AT-STOP: x0={:#x} x1={:#x} x2={:#x} x9={:#x}  (x0 device: {:?})",
            rx[0], rx[1], rx[2], rx[3], machine::device_at(rx[0])
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

        hv_vcpu_destroy(vcpu);
        hv_vm_destroy();

        println!("=== EDK2 boot probe (with Apple hv_gic) ===");
        println!("stop: {stop_reason}");
        println!("exits: {exits} (vtimer {vtimer_exits}, psci {psci_calls}), last PC: {last_pc:#x}");
        println!("unmodelled MMIO touched: {unimpl:?}");
        if redist_hi != 0 {
            println!(
                "gic-redist IPA range: {redist_lo:#x}..={redist_hi:#x} (redist base {:#x}, frame0 ends {:#x})",
                machine::GIC_REDIST.base,
                machine::GIC_REDIST.base + 0x20000
            );
        }
        println!("serial bytes: {}", serial.len());
        println!("--- serial (tail) ---\n{}\n--- end ---", String::from_utf8_lossy(&serial));
    }
}
