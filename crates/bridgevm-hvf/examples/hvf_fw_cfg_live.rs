//! LIVE end-to-end proof: a real guest vCPU performs an MMIO read of the
//! `fw_cfg` signature; the host run loop catches the stage-2 data abort, decodes
//! the ISS, and routes it through [`VirtPlatform::on_mmio`] -> [`crate::fwcfg`],
//! feeding the byte back into the guest register. Confirms the Path A platform
//! works against actual Hypervisor.framework, not just unit tests.
//!
//! This needs the `com.apple.security.hypervisor` entitlement, so it cannot run
//! under plain `cargo run`. Build, ad-hoc sign, then run the produced binary:
//!
//! ```sh
//! cargo build -p bridgevm-hvf --example hvf_fw_cfg_live
//! codesign --sign - --entitlements hv.entitlements --force \
//!   target/debug/examples/hvf_fw_cfg_live
//! target/debug/examples/hvf_fw_cfg_live
//! ```

use std::alloc::{alloc_zeroed, Layout};
use std::os::raw::c_void;
use std::ptr::null_mut;

use bridgevm_hvf::dtb::VirtFdtConfig;
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
    fn hv_vcpu_create(vcpu: *mut HvVcpuT, exit: *mut *mut HvVcpuExit, config: *mut c_void) -> HvReturn;
    fn hv_vcpu_destroy(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpu_run(vcpu: HvVcpuT) -> HvReturn;
    fn hv_vcpu_get_reg(vcpu: HvVcpuT, reg: u32, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_reg(vcpu: HvVcpuT, reg: u32, value: u64) -> HvReturn;
}

const HV_REG_X0: u32 = 0;
const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const EXIT_EXCEPTION: u32 = 1;
const EC_DATA_ABORT: u64 = 0x24;
const EC_HVC: u64 = 0x16;
const GUEST_BASE: u64 = 0x4000_0000;

fn main() {
    // Guest: mov x1,#0x09020000 (fw_cfg DATA) ; ldrb w0,[x1] ; hvc #0
    let code: [u32; 3] = [0xd2a1_2041, 0x3940_0020, 0xd400_0002];

    unsafe {
        assert_eq!(hv_vm_create(null_mut()), 0, "hv_vm_create");

        let size = 0x20_0000usize;
        let layout = Layout::from_size_align(size, 0x1_0000).unwrap();
        let mem = alloc_zeroed(layout);
        assert!(!mem.is_null(), "alloc");
        for (i, w) in code.iter().enumerate() {
            std::ptr::copy_nonoverlapping(w.to_le_bytes().as_ptr(), mem.add(i * 4), 4);
        }
        assert_eq!(
            hv_vm_map(
                mem as *mut c_void,
                GUEST_BASE,
                size,
                HV_MEMORY_READ | HV_MEMORY_WRITE | HV_MEMORY_EXEC
            ),
            0,
            "hv_vm_map"
        );

        let mut vcpu: HvVcpuT = 0;
        let mut exit: *mut HvVcpuExit = null_mut();
        assert_eq!(hv_vcpu_create(&mut vcpu, &mut exit, null_mut()), 0, "hv_vcpu_create");
        hv_vcpu_set_reg(vcpu, HV_REG_PC, GUEST_BASE);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5); // EL1h, DAIF masked

        let mut platform = VirtPlatform::new(VirtFdtConfig::default());
        let mut guest_ram = FlatGuestRam::new(GUEST_BASE, 0); // signature read needs no DMA
        let mut mmio_reads = 0u32;

        loop {
            assert_eq!(hv_vcpu_run(vcpu), 0, "hv_vcpu_run");
            let reason = (*exit).reason;
            if reason != EXIT_EXCEPTION {
                println!("unexpected exit reason {reason}");
                break;
            }
            let esr = (*exit).exception.syndrome;
            let ec = (esr >> 26) & 0x3f;
            match ec {
                EC_DATA_ABORT => {
                    let ipa = (*exit).exception.physical_address;
                    let sas = ((esr >> 22) & 0x3) as u8;
                    let size = 1u8 << sas;
                    let srt = ((esr >> 16) & 0x1f) as u32;
                    let is_write = (esr >> 6) & 1 == 1;
                    let mut pc = 0u64;
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
                    if is_write {
                        let mut v = 0u64;
                        hv_vcpu_get_reg(vcpu, HV_REG_X0 + srt, &mut v);
                        let _ = platform.on_mmio(ipa, MmioOp::Write { size, value: v }, &mut guest_ram);
                    } else {
                        match platform.on_mmio(ipa, MmioOp::Read { size }, &mut guest_ram) {
                            MmioOutcome::ReadValue(v) => {
                                hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                                mmio_reads += 1;
                                println!("MMIO read  @ {ipa:#011x} size {size} -> {v:#04x} into x{srt}");
                            }
                            other => println!("on_mmio @ {ipa:#x} -> {other:?}"),
                        }
                    }
                    hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
                }
                EC_HVC => {
                    println!("guest HVC -> run loop done");
                    break;
                }
                _ => {
                    println!("unexpected EC {ec:#x} (ESR {esr:#x})");
                    break;
                }
            }
        }

        let mut x0 = 0u64;
        hv_vcpu_get_reg(vcpu, HV_REG_X0, &mut x0);
        hv_vcpu_destroy(vcpu);
        hv_vm_destroy();

        println!("guest X0   = {x0:#04x} ('{}')", x0 as u8 as char);
        assert_eq!(mmio_reads, 1, "exactly one fw_cfg MMIO read expected");
        assert_eq!(x0, 0x51, "guest must observe fw_cfg signature byte 'Q' (0x51)");
        println!(
            "LIVE PROOF: real guest MMIO -> VirtPlatform::on_mmio -> fw_cfg -> guest saw 'Q' (0x51)"
        );
    }
}
