//! Bridge between the probe's run loops and the userspace GICv3.
//!
//! `BRIDGEVM_USERSPACE_GIC=1` swaps Apple's in-kernel `hv_gic` for the
//! `bridgevm_hvf::userspace_gic` device model — the configuration QEMU-hvf
//! proved boots this host 10/10 while the in-kernel GIC measures 14/40
//! (docs/windows-arm/evidence/a1-qemu-userspace-gic-control-20260808.md).
//!
//! Contract with the run loops (QEMU parity):
//! - GICD/GICR/GICv2m data aborts and trapped ICC_* sysregs route here;
//! - every `hv_vcpu_run` is preceded by `pre_run` (vtimer level sync +
//!   `hv_vcpu_set_pending_interrupt`, which HVF consumes per run);
//! - vtimer exits mask the HVF vtimer and latch the PPI level; the level
//!   drops in `pre_run` once CNTV_CTL stops asserting (ENABLE&&ISTATUS&&!IMASK),
//!   which is when the vtimer is unmasked again;
//! - WFI traps (the kernel GIC used to absorb them) poll the line and nap;
//! - cross-CPU state changes return kick masks -> `hv_vcpus_exit`.

use crate::*;
use bridgevm_hvf::userspace_gic::UserspaceGic;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::Duration;

extern "C" {
    fn mach_absolute_time() -> u64;
}

/// Host timebase ticks (mach_absolute_time) for callers outside this module.
pub(crate) fn host_time_ticks() -> u64 {
    unsafe { mach_absolute_time() }
}

const MAX_VCPUS: usize = 16;

pub(crate) struct UsGicBridge {
    gic: Mutex<UserspaceGic>,
    num_cpus: usize,
    handles: [AtomicU64; MAX_VCPUS],
    registered: [AtomicBool; MAX_VCPUS],
    vtimer_masked: [AtomicBool; MAX_VCPUS],
    trace_budget: AtomicU64,
    /// Ring of the most recent GIC ops (BRIDGEVM_USGIC_RING=1); dumped on
    /// SYSTEM_RESET and in stall reports. A fixed println budget always
    /// ran out before the interesting event.
    ring: Mutex<std::collections::VecDeque<String>>,
    ring_enabled: bool,
}

macro_rules! usgic_trace {
    ($bridge:expr, $($arg:tt)*) => {
        if $bridge.trace_budget.load(Ordering::Relaxed) > 0
            && $bridge.trace_budget.fetch_sub(1, Ordering::Relaxed) > 0
        {
            println!($($arg)*);
        }
    };
}

static BRIDGE: OnceLock<Option<UsGicBridge>> = OnceLock::new();

/// Called once from `create_gic`. Returns true when the userspace GIC is
/// active (the in-kernel GIC must then NOT be created).
pub(crate) fn init_if_enabled(num_cpus: usize) -> bool {
    BRIDGE
        .get_or_init(|| env_flag("BRIDGEVM_USERSPACE_GIC").then(|| UsGicBridge::new(num_cpus)))
        .is_some()
}

pub(crate) fn usgic() -> Option<&'static UsGicBridge> {
    BRIDGE.get().and_then(|bridge| bridge.as_ref())
}

pub(crate) fn register_vcpu(cpu: usize, vcpu: HvVcpuT) {
    if let Some(bridge) = usgic() {
        bridge.handles[cpu].store(vcpu, Ordering::SeqCst);
        bridge.registered[cpu].store(true, Ordering::SeqCst);
        // Apple silicon has no GICv3 CPU interface, so ID_AA64PFR0_EL1.GIC
        // reads 0 without the in-kernel GIC and guests never issue ICC_*
        // accesses (EDK2/Windows check this field before selecting the
        // sysreg interface). Advertise GIC=1; the unallocated ICC encodings
        // then trap to us as EC_SYS_REG_TRAP (same contract QEMU-hvf uses).
        unsafe {
            let mut pfr0 = 0u64;
            if hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_ID_AA64PFR0_EL1, &mut pfr0) == 0 {
                let shaped = (pfr0 & !(0xf << 24)) | (1 << 24);
                let status = hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_ID_AA64PFR0_EL1, shaped);
                let mut after = 0u64;
                hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_ID_AA64PFR0_EL1, &mut after);
                println!(
                    "USGIC cpu{cpu} PFR0.GIC: before={pfr0:#x} set={status:#x} after={after:#x}"
                );
            }
        }
    }
}

/// Pre-`hv_vcpu_run` refresh. No-op when the userspace GIC is disabled.
pub(crate) fn pre_run(cpu: usize, vcpu: HvVcpuT) {
    if let Some(bridge) = usgic() {
        unsafe { bridge.pre_run(cpu, vcpu) };
    }
}

/// EXIT_VTIMER handling for both modes: in-kernel mode masks the vtimer
/// (the kernel GIC already latched the PPI); userspace mode masks AND
/// latches the PPI level into the emulated redistributor.
pub(crate) fn vtimer_exit_mask(cpu: usize, vcpu: HvVcpuT) {
    unsafe {
        hv_vcpu_set_vtimer_mask(vcpu, true);
    }
    if let Some(bridge) = usgic() {
        bridge.vtimer_masked[cpu].store(true, Ordering::SeqCst);
        let kicks = bridge.gic.lock().unwrap().set_vtimer_ppi(cpu, true);
        bridge.ring_push(format!("c{cpu} vtimer-fire k={kicks:#x}"));
        bridge.kick(kicks, cpu);
    }
}

/// Trapped ICC_* access. Returns true when consumed (PC already advanced).
pub(crate) fn try_sysreg(cpu: usize, vcpu: HvVcpuT, esr: u64, pc: u64) -> bool {
    let Some(bridge) = usgic() else {
        return false;
    };
    unsafe { bridge.sysreg_trap(cpu, vcpu, esr, pc) }
}

/// GICD/GICR/MSI-frame data abort. Returns true when consumed (PC advanced).
pub(crate) fn try_data_abort(
    cpu: usize,
    vcpu: HvVcpuT,
    ipa: u64,
    op: &MmioOp,
    srt: u32,
    pc: u64,
) -> bool {
    let Some(bridge) = usgic() else {
        return false;
    };
    if !UserspaceGic::owns(ipa) {
        return false;
    }
    unsafe { bridge.mmio_abort(cpu, vcpu, ipa, op, srt, pc) };
    true
}

/// Trapped WFI/WFE (only taken without the in-kernel GIC). Advances PC; naps
/// briefly on WFI when no interrupt is deliverable so idle guests do not
/// busy-spin the host.
pub(crate) fn wfx_trap(cpu: usize, vcpu: HvVcpuT, esr: u64, pc: u64) -> bool {
    let Some(bridge) = usgic() else {
        return false;
    };
    let is_wfe = esr & 1 != 0;
    if !is_wfe {
        let line = {
            let gic = bridge.gic.lock().unwrap();
            gic.line_asserted(cpu)
        };
        if !line {
            std::thread::sleep(Duration::from_micros(500));
        }
    }
    unsafe {
        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
    }
    true
}

/// Device SPI level -> userspace distributor (replaces `hv_gic_set_spi`).
pub(crate) fn deliver_spi(intid: u32, level: bool) -> Option<HvReturn> {
    let bridge = usgic()?;
    let kicks = bridge.gic.lock().unwrap().set_spi(intid, level);
    bridge.ring_push(format!("spi {intid} lvl={} k={kicks:#x}", u8::from(level)));
    bridge.kick(kicks, usize::MAX);
    Some(0)
}

/// MSI doorbell write -> userspace distributor (replaces `hv_gic_send_msi`).
pub(crate) fn deliver_msi(address: u64, data: u32) -> Option<HvReturn> {
    let bridge = usgic()?;
    let kicks = bridge.gic.lock().unwrap().send_msi(address, data);
    bridge.ring_push(format!("msi {address:#x} intid={data} k={kicks:#x}"));
    bridge.kick(kicks, usize::MAX);
    Some(0)
}

/// Stall diagnostics: dump per-CPU GIC + vtimer state (watchdog reports).
pub(crate) fn stall_report(vcpu0: HvVcpuT) {
    let Some(bridge) = usgic() else {
        return;
    };
    let gic = bridge.gic.lock().unwrap();
    for cpu in 0..bridge.num_cpus {
        println!("USGIC stall cpu{cpu}: {}", gic.debug_line(cpu));
    }
    drop(gic);
    bridge.ring_dump("stall");
    unsafe {
        let mut ctl = 0u64;
        let mut cval = 0u64;
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_CNTV_CTL_EL0, &mut ctl);
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_CNTV_CVAL_EL0, &mut cval);
        println!(
            "USGIC stall cpu0 vtimer: ctl={ctl:#x} cval={cval:#x} now={:#x} host_masked={}",
            mach_absolute_time(),
            bridge.vtimer_masked[0].load(Ordering::SeqCst)
        );
        // The guest-visible exception state: if the guest parked in its own
        // vector (b .), ESR_EL1/ELR_EL1/FAR_EL1 name the exception that put
        // it there.
        let mut esr = 0u64;
        let mut elr = 0u64;
        let mut far = 0u64;
        let mut vbar = 0u64;
        let mut spsr = 0u64;
        let mut cpsr = 0u64;
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_ESR_EL1, &mut esr);
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_ELR_EL1, &mut elr);
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_FAR_EL1, &mut far);
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_VBAR_EL1, &mut vbar);
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_SPSR_EL1, &mut spsr);
        hv_vcpu_get_reg(vcpu0, HV_REG_CPSR, &mut cpsr);
        println!(
            "USGIC stall cpu0 guest-exception: ESR_EL1={esr:#x} (EC={:#x}) ELR_EL1={elr:#x} FAR_EL1={far:#x} VBAR_EL1={vbar:#x} SPSR_EL1={spsr:#x} PSTATE={cpsr:#x}",
            (esr >> 26) & 0x3f
        );
    }
}

/// SYSTEM_RESET replacement for `hv_gic_reset` in userspace mode.
pub(crate) fn reset() -> bool {
    let Some(bridge) = usgic() else {
        return false;
    };
    bridge.ring_dump("SYSTEM_RESET");
    *bridge.gic.lock().unwrap() = UserspaceGic::new(bridge.num_cpus);
    // The HOST vtimer mask survives guest resets: a reboot that lands while
    // the PPI is latched (mask=true, EOI never came) would leave the next
    // generation without a single EXIT_VTIMER -- BdsDxe then spins on a
    // timer that never fires (p1gate-20260808 boot-1, TianoCore spinner
    // frozen 18 min). Unmask every registered vCPU at reset.
    for cpu in 0..bridge.num_cpus.min(MAX_VCPUS) {
        bridge.vtimer_masked[cpu].store(false, Ordering::SeqCst);
        if bridge.registered[cpu].load(Ordering::SeqCst) {
            let handle = bridge.handles[cpu].load(Ordering::SeqCst);
            unsafe {
                hv_vcpu_set_vtimer_mask(handle, false);
            }
        }
    }
    true
}

impl UsGicBridge {
    fn new(num_cpus: usize) -> Self {
        Self {
            gic: Mutex::new(UserspaceGic::new(num_cpus)),
            num_cpus,
            handles: [const { AtomicU64::new(0) }; MAX_VCPUS],
            registered: [const { AtomicBool::new(false) }; MAX_VCPUS],
            vtimer_masked: [const { AtomicBool::new(false) }; MAX_VCPUS],
            trace_budget: AtomicU64::new(env_u64("BRIDGEVM_USGIC_TRACE", 0)),
            ring: Mutex::new(std::collections::VecDeque::with_capacity(256)),
            ring_enabled: env_flag("BRIDGEVM_USGIC_RING"),
        }
    }

    fn ring_push(&self, line: String) {
        if !self.ring_enabled {
            return;
        }
        let mut ring = self.ring.lock().unwrap();
        if ring.len() >= 256 {
            ring.pop_front();
        }
        ring.push_back(line);
    }

    pub(crate) fn ring_dump(&self, reason: &str) {
        if !self.ring_enabled {
            return;
        }
        let ring = self.ring.lock().unwrap();
        println!("USGIC ring dump ({reason}): {} entries", ring.len());
        for line in ring.iter() {
            println!("USGIC-R {line}");
        }
    }

    fn kick(&self, mask: u64, self_cpu: usize) {
        if mask == 0 {
            return;
        }
        for cpu in 0..self.num_cpus.min(MAX_VCPUS) {
            if cpu == self_cpu || mask & (1u64 << cpu) == 0 {
                continue;
            }
            if !self.registered[cpu].load(Ordering::SeqCst) {
                continue;
            }
            let handle = self.handles[cpu].load(Ordering::SeqCst);
            unsafe {
                hv_vcpus_exit(&handle, 1);
            }
        }
    }

    unsafe fn pre_run(&self, cpu: usize, vcpu: HvVcpuT) {
        // The vtimer PPI is edge-latched at EXIT_VTIMER and consumed by the
        // guest's IAR read; the host vtimer stays masked until the guest
        // EOIs INTID 27 (see sysreg_trap). Nothing to sync here beyond the
        // IRQ line itself.
        let line = self.gic.lock().unwrap().line_asserted(cpu);
        hv_vcpu_set_pending_interrupt(vcpu, HV_INTERRUPT_TYPE_IRQ, line);
    }

    unsafe fn sysreg_trap(&self, cpu: usize, vcpu: HvVcpuT, esr: u64, pc: u64) -> bool {
        let trap = SysRegTrap::decode(esr);
        let encoding = (u16::from(trap.op0) << 14)
            | (u16::from(trap.op1) << 11)
            | (u16::from(trap.crn) << 7)
            | (u16::from(trap.crm) << 3)
            | u16::from(trap.op2);
        let write_value = if trap.is_read || trap.rt == 31 {
            0
        } else {
            let mut value = 0u64;
            hv_vcpu_get_reg(vcpu, HV_REG_X0 + trap.rt, &mut value);
            value
        };
        let result = self
            .gic
            .lock()
            .unwrap()
            .sysreg(cpu, encoding, trap.is_read, write_value);
        usgic_trace!(
            self,
            "USGIC cpu{cpu} sysreg {}{encoding:#x} val={write_value:#x} handled={} pc={pc:#x}",
            if trap.is_read { "read " } else { "write " },
            result.is_some()
        );
        if self.ring_enabled {
            if let Some(r) = result {
                self.ring_push(format!(
                    "c{cpu} {}{encoding:#x} w={write_value:#x} -> {:#x} k={:#x} pc={pc:#x}",
                    if trap.is_read { "r" } else { "W" },
                    r.value,
                    r.kick_mask
                ));
            }
        }
        let Some(result) = result else {
            return false;
        };
        if trap.is_read && trap.rt != 31 {
            hv_vcpu_set_reg(vcpu, HV_REG_X0 + trap.rt, result.value);
        }
        // Guest completed the vtimer interrupt: re-enable host fire exits.
        // (EOIR1 in EOImode=0, or DIR in EOImode=1, naming INTID 27.)
        if !trap.is_read
            && (encoding == bridgevm_hvf::userspace_gic::ICC_EOIR1_EL1
                || encoding == bridgevm_hvf::userspace_gic::ICC_DIR_EL1)
            && write_value & 0xff_ffff == u64::from(bridgevm_hvf::userspace_gic::VTIMER_INTID)
            && self.vtimer_masked[cpu].swap(false, Ordering::SeqCst)
        {
            hv_vcpu_set_vtimer_mask(vcpu, false);
        }
        self.kick(result.kick_mask, cpu);
        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
        true
    }

    unsafe fn mmio_abort(
        &self,
        cpu: usize,
        vcpu: HvVcpuT,
        ipa: u64,
        op: &MmioOp,
        srt: u32,
        pc: u64,
    ) {
        let (width, write) = match *op {
            MmioOp::Read { size } => (size, None),
            MmioOp::Write { size, value } => (size, Some(value)),
        };
        let result = self.gic.lock().unwrap().mmio(ipa, width, write);
        if self.ring_enabled {
            self.ring_push(format!(
                "c{cpu} mmio {ipa:#x} w{width} wr={write:?} -> {:#x} k={:#x}",
                result.value, result.kick_mask
            ));
        }
        usgic_trace!(
            self,
            "USGIC cpu{cpu} mmio ipa={ipa:#x} w={width} write={write:?} -> {:#x} pc={pc:#x}",
            result.value
        );
        if write.is_none() && srt != 31 {
            let mask = if width >= 8 {
                u64::MAX
            } else {
                (1u64 << (u32::from(width) * 8)) - 1
            };
            hv_vcpu_set_reg(vcpu, HV_REG_X0 + srt, result.value & mask);
        }
        self.kick(result.kick_mask, cpu);
        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
    }
}
