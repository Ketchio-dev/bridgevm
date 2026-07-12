//! Bounded one-shot probe: load the real ArmVirtQemu firmware
//! (`edk2-aarch64-code.fd`) onto the Path A `virt` platform and run it, capturing
//! PL011 serial output and recording the first unmodelled device it touches.
//! The goal is to find the *next blocker*, not to fully boot — a watchdog and an
//! exit cap keep it bounded.
//!
//! Build, ad-hoc sign, run (needs `com.apple.security.hypervisor`):
//!   cargo build -p bridgevm-hvf --example hvf_edk2_boot_probe
//!   codesign --sign - --entitlements hv.entitlements --force target/debug/examples/hvf_edk2_boot_probe
//!   BRIDGEVM_AARCH64_UEFI_CODE=.../edk2-aarch64-code.fd \
//!   BRIDGEVM_AARCH64_UEFI_VARS=.../edk2-arm-vars.fd \
//!   target/debug/examples/hvf_edk2_boot_probe

use std::alloc::{alloc_zeroed, Layout};
use std::collections::BTreeMap;
use std::os::raw::c_void;
use std::ptr::null_mut;

use bridgevm_hvf::dtb::{build_virt_fdt, VirtFdtConfig};
use bridgevm_hvf::machine;
use bridgevm_hvf::platform_virt::{FlatGuestRam, MmioOp, MmioOutcome, VirtPlatform};

type HvReturn = i32;
type HvVcpuT = u64;

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
}

const HV_REG_X0: u32 = 0;
const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const EXIT_CANCELED: u32 = 0;
const EXIT_EXCEPTION: u32 = 1;
const EC_DATA_ABORT: u64 = 0x24;
const EC_HVC: u64 = 0x16;

const RAM_SIZE: usize = 0x2000_0000; // 512 MiB
const MAX_EXITS: u64 = 2_000_000;
const WATCHDOG_MS: u64 = 4000;

fn map_file(path: &str, ipa: u64, region_bytes: usize, flags: u64) {
    let data = bridgevm_hvf::media::read_bounded_file(path, region_bytes)
        .unwrap_or_else(|e| panic!("read {path}: {e}"));
    let layout = Layout::from_size_align(region_bytes, 0x1_0000).unwrap();
    unsafe {
        let mem = alloc_zeroed(layout);
        std::ptr::copy_nonoverlapping(data.as_ptr(), mem, data.len());
        assert_eq!(
            hv_vm_map(mem as *mut c_void, ipa, region_bytes, flags),
            0,
            "map {path}"
        );
    }
}

fn main() {
    let code = std::env::var("BRIDGEVM_AARCH64_UEFI_CODE").unwrap_or_else(|_| {
        "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-aarch64-code.fd".into()
    });
    let vars = std::env::var("BRIDGEVM_AARCH64_UEFI_VARS")
        .unwrap_or_else(|_| "/opt/homebrew/Cellar/qemu/11.0.1/share/qemu/edk2-arm-vars.fd".into());

    unsafe {
        assert_eq!(hv_vm_create(null_mut()), 0, "hv_vm_create");

        // Flash code (RX) at 0x0, flash vars (RW) at 0x04000000.
        map_file(
            &code,
            machine::FLASH_CODE.base,
            machine::FLASH_CODE.size as usize,
            HV_MEMORY_READ | HV_MEMORY_EXEC,
        );
        map_file(
            &vars,
            machine::FLASH_VARS.base,
            machine::FLASH_VARS.size as usize,
            HV_MEMORY_READ | HV_MEMORY_WRITE,
        );

        // RAM at 0x40000000, with the device tree at its base (DRAM base = where
        // ArmVirtQemu looks for the DTB).
        let ram_layout = Layout::from_size_align(RAM_SIZE, 0x1_0000).unwrap();
        let ram = alloc_zeroed(ram_layout);
        let dtb = build_virt_fdt(&VirtFdtConfig {
            cpu_count: 1,
            ram_size: RAM_SIZE as u64,
        });
        std::ptr::copy_nonoverlapping(dtb.as_ptr(), ram, dtb.len());
        assert_eq!(
            hv_vm_map(
                ram as *mut c_void,
                machine::RAM_BASE,
                RAM_SIZE,
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
        hv_vcpu_set_reg(vcpu, HV_REG_PC, 0x0); // reset vector
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5); // EL1h, DAIF masked
        hv_vcpu_set_reg(vcpu, HV_REG_X0, machine::RAM_BASE); // DTB pointer (boot protocol)

        // Watchdog: force the vCPU out of hv_vcpu_run after WATCHDOG_MS.
        let vcpu_for_wd = vcpu;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(WATCHDOG_MS));
            let v = vcpu_for_wd;
            hv_vcpus_exit(&v, 1);
        });

        let mut platform = VirtPlatform::new(VirtFdtConfig {
            cpu_count: 1,
            ram_size: RAM_SIZE as u64,
        });
        let mut guest_ram = FlatGuestRam::new(machine::RAM_BASE, 0);
        let mut unimpl: BTreeMap<&'static str, u64> = BTreeMap::new();
        let mut exits = 0u64;
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
                        MmioOutcome::ReadValue(v) => {
                            if !is_write {
                                hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                            }
                        }
                        MmioOutcome::WriteAck => {}
                        MmioOutcome::KnownUnimplemented(name) => {
                            *unimpl.entry(name).or_insert(0) += 1;
                            // Return 0 for reads so the firmware keeps moving.
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
                    stop_reason = "guest HVC".into();
                    break;
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
        hv_vcpu_destroy(vcpu);
        hv_vm_destroy();

        println!("=== EDK2 boot probe result ===");
        println!("stop: {stop_reason}");
        println!("exits: {exits}, last PC: {last_pc:#x}");
        println!("unmodelled MMIO touched: {unimpl:?}");
        println!("serial bytes captured: {}", serial.len());
        if !serial.is_empty() {
            println!(
                "--- serial ---\n{}\n--- end serial ---",
                String::from_utf8_lossy(&serial)
            );
        } else {
            println!("(no serial output captured)");
        }
    }
}
