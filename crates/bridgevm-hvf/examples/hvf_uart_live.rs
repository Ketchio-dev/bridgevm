//! LIVE proof: a real guest writes bytes to the PL011 UART; the host run loop
//! routes the MMIO stores through [`VirtPlatform::on_mmio`] -> [`crate::pl011`]
//! and captures the serial output. Confirms guest-visible serial works on real
//! Hypervisor.framework — the prerequisite for observing firmware/OS bring-up.
//!
//! Build, ad-hoc sign, run (needs `com.apple.security.hypervisor`):
//!   cargo build -p bridgevm-hvf --example hvf_uart_live
//!   codesign --sign - --entitlements hv.entitlements --force target/debug/examples/hvf_uart_live
//!   target/debug/examples/hvf_uart_live

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
    fn hv_vcpu_create(
        vcpu: *mut HvVcpuT,
        exit: *mut *mut HvVcpuExit,
        config: *mut c_void,
    ) -> HvReturn;
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
    // mov x1,#0x09000000 (UARTDR); for each of 'H','I','\n': mov w0,#c; strb w0,[x1]; then hvc #0
    let code: [u32; 8] = [
        0xd2a1_2001, // mov x1, #0x09000000
        0x5280_0900, // mov w0, #0x48 'H'
        0x3900_0020, // strb w0, [x1]
        0x5280_0920, // mov w0, #0x49 'I'
        0x3900_0020, // strb w0, [x1]
        0x5280_0140, // mov w0, #0x0a '\n'
        0x3900_0020, // strb w0, [x1]
        0xd400_0002, // hvc #0
    ];

    unsafe {
        assert_eq!(hv_vm_create(null_mut()), 0, "hv_vm_create");
        let size = 0x20_0000usize;
        let layout = Layout::from_size_align(size, 0x1_0000).unwrap();
        let mem = alloc_zeroed(layout);
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
        assert_eq!(
            hv_vcpu_create(&mut vcpu, &mut exit, null_mut()),
            0,
            "hv_vcpu_create"
        );
        hv_vcpu_set_reg(vcpu, HV_REG_PC, GUEST_BASE);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5);

        let mut platform = VirtPlatform::new(VirtFdtConfig::default());
        let mut guest_ram = FlatGuestRam::new(GUEST_BASE, 0);

        loop {
            assert_eq!(hv_vcpu_run(vcpu), 0, "hv_vcpu_run");
            if (*exit).reason != EXIT_EXCEPTION {
                println!("unexpected exit reason {}", (*exit).reason);
                break;
            }
            let esr = (*exit).exception.syndrome;
            let ec = (esr >> 26) & 0x3f;
            match ec {
                EC_DATA_ABORT => {
                    let ipa = (*exit).exception.physical_address;
                    let size = 1u8 << ((esr >> 22) & 0x3);
                    let srt = ((esr >> 16) & 0x1f) as u32;
                    let is_write = (esr >> 6) & 1 == 1;
                    let mut pc = 0u64;
                    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
                    if is_write {
                        let mut v = 0u64;
                        hv_vcpu_get_reg(vcpu, HV_REG_X0 + srt, &mut v);
                        let _ =
                            platform.on_mmio(ipa, MmioOp::Write { size, value: v }, &mut guest_ram);
                    } else if let MmioOutcome::ReadValue(v) =
                        platform.on_mmio(ipa, MmioOp::Read { size }, &mut guest_ram)
                    {
                        hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, v);
                    }
                    hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
                }
                EC_HVC => break,
                _ => {
                    println!("unexpected EC {ec:#x}");
                    break;
                }
            }
        }

        let out = platform.uart_output().to_vec();
        hv_vcpu_destroy(vcpu);
        hv_vm_destroy();

        println!("UART captured: {:?}", String::from_utf8_lossy(&out));
        assert_eq!(out, b"HI\n", "guest serial output must be 'HI\\n'");
        println!("LIVE PROOF: real guest UART writes -> VirtPlatform -> captured serial 'HI'");
    }
}
