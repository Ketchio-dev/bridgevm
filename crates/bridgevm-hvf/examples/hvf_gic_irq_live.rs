//! LIVE proof that the interrupt + architected-timer path works end to end through
//! Apple `hv_gic`: a minimal EL1 guest configures the GICv3 CPU interface, enables
//! the virtual-timer PPI (intid 27) in its redistributor, installs an EL1 vector
//! table, arms `CNTV` for "now + ~1 ms", enables IRQs, and spins. Apple's in-kernel
//! GIC delivers the timer interrupt (no `VTIMER_ACTIVATED` exit), the guest's IRQ
//! handler runs and writes a flag to guest RAM, which the host reads back.
//!
//! This isolates the interrupt subsystem from the full-firmware bring-up: it proves
//! the timer fires, the PPI is routed by `hv_gic`, and the IRQ is taken at EL1.
//!
//! Build, ad-hoc sign, run (needs `com.apple.security.hypervisor`):
//!   cargo build -p bridgevm-hvf --example hvf_gic_irq_live
//!   codesign --sign - --entitlements hv.entitlements --force target/debug/examples/hvf_gic_irq_live
//!   target/debug/examples/hvf_gic_irq_live

use std::alloc::{alloc_zeroed, Layout};
use std::os::raw::c_void;
use std::ptr::null_mut;

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
    fn hv_vcpu_set_reg(vcpu: HvVcpuT, reg: u32, value: u64) -> HvReturn;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpuT, reg: u16, value: u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpuT, vtimer_is_masked: bool) -> HvReturn;
    fn hv_gic_config_create() -> HvGicConfig;
    fn hv_gic_config_set_distributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_config_set_redistributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_create(config: HvGicConfig) -> HvReturn;
}

const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const GUEST_BASE: u64 = 0x4000_0000;
const FLAG_OFFSET: usize = 0x3000;

// Guest setup: GIC CPU interface (ICC_SRE/PMR/IGRPEN1), VBAR_EL1=0x40001000,
// GICD_CTLR, redistributor SGI frame (IGROUPR0/ISENABLER0/IPRIORITYR for PPI 27),
// arm CNTV = now + 0x100000, enable IRQs, spin. (Assembled from gic-timer asm.)
const SETUP: [u32; 29] = [
    0xd2800020, 0xd518cca0, 0xd5033fdf, 0xd2801fe0, 0xd5184600, 0xd2800020, 0xd518cce0, 0xd2a80000,
    0xf2820000, 0xd518c000, 0xd5033fdf, 0xd2a10001, 0x52800262, 0xb9000022, 0xd2a10161, 0x52a10002,
    0xb9008022, 0xb9010022, 0xb904183f, 0xd5033f9f, 0xd5033fdf, 0xd53be040, 0xd2a00203, 0x8b030000,
    0xd51be340, 0x52800020, 0xd51be320, 0xd50342ff, 0x14000000,
];
// IRQ handler (placed at VBAR + 0x280): ack ICC_IAR1_EL1, write flag at 0x40003000,
// EOI, eret.
const HANDLER: [u32; 7] = [
    0xd538cc00, 0xd2a80001, 0xf2860001, 0xd2800022, 0xf9000022, 0xd518cc20, 0xd69f03e0,
];

fn main() {
    unsafe {
        assert_eq!(hv_vm_create(null_mut()), 0, "hv_vm_create");
        let gic = hv_gic_config_create();
        hv_gic_config_set_distributor_base(gic, 0x0800_0000);
        hv_gic_config_set_redistributor_base(gic, 0x080a_0000);
        assert_eq!(hv_gic_create(gic), 0, "hv_gic_create");

        let layout = Layout::from_size_align(0x1_0000, 0x1_0000).unwrap();
        let mem = alloc_zeroed(layout);
        for (i, w) in SETUP.iter().enumerate() {
            std::ptr::copy_nonoverlapping(w.to_le_bytes().as_ptr(), mem.add(i * 4), 4);
        }
        for (i, w) in HANDLER.iter().enumerate() {
            std::ptr::copy_nonoverlapping(w.to_le_bytes().as_ptr(), mem.add(0x1280 + i * 4), 4);
        }
        assert_eq!(
            hv_vm_map(
                mem as *mut c_void,
                GUEST_BASE,
                0x1_0000,
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
        hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_MPIDR_EL1, 0x8000_0000);
        hv_vcpu_set_reg(vcpu, HV_REG_PC, GUEST_BASE);
        hv_vcpu_set_reg(vcpu, HV_REG_CPSR, 0x3c5);
        hv_vcpu_set_vtimer_mask(vcpu, false);

        let vcpu_for_wd = vcpu;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(2000));
            let v = vcpu_for_wd;
            hv_vcpus_exit(&v, 1);
        });

        let mut vtimer_exits = 0u32;
        loop {
            assert_eq!(hv_vcpu_run(vcpu), 0, "hv_vcpu_run");
            match (*exit).reason {
                0 => break, // CANCELED (watchdog)
                2 => {
                    // VTIMER_ACTIVATED should NOT happen with hv_gic (in-kernel delivery).
                    vtimer_exits += 1;
                    hv_vcpu_set_vtimer_mask(vcpu, true);
                    if vtimer_exits > 3 {
                        break;
                    }
                }
                1 => {
                    let esr = (*exit).exception.syndrome;
                    println!(
                        "unexpected exception EC {:#x} ESR {esr:#x}",
                        (esr >> 26) & 0x3f
                    );
                    break;
                }
                r => {
                    println!("unexpected exit reason {r}");
                    break;
                }
            }
        }

        let flag = *(mem.add(FLAG_OFFSET) as *const u32);
        hv_vcpu_destroy(vcpu);
        hv_vm_destroy();

        println!("flag={flag} vtimer_exits={vtimer_exits}");
        assert_eq!(
            flag, 1,
            "guest IRQ handler must have run (timer PPI delivered)"
        );
        assert_eq!(
            vtimer_exits, 0,
            "with hv_gic the timer is delivered in-kernel (no VTIMER exit)"
        );
        println!(
            "LIVE PROOF: hv_gic delivers the architected-timer PPI to an EL1 guest IRQ handler"
        );
    }
}
