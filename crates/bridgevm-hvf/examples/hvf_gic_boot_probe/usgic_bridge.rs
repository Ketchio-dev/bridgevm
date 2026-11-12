//! Bridge between the probe's run loops and the userspace GICv3.
//!
//! `BRIDGEVM_USERSPACE_GIC=1` swaps Apple's in-kernel `hv_gic` for the
//! `bridgevm_hvf::userspace_gic` device model. Retained live transition
//! evidence is in `a1-userspace-gic-swap-20260808.md`.
//!
//! Contract with the run loops:
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
use std::sync::{Condvar, Mutex, OnceLock};
use std::time::Duration;

extern "C" {
    fn mach_absolute_time() -> u64;
}

/// Host timebase ticks (mach_absolute_time) for callers outside this module.
pub(crate) fn host_time_ticks() -> u64 {
    unsafe { mach_absolute_time() }
}

fn timer_expired(cval: u64, offset: u64, counters: &impl crate::pause_gate::HostCounters) -> bool {
    cval <= counters.mach().wrapping_sub(offset)
}

#[cfg(test)]
mod clock_tests {
    use super::*;
    struct AfterSleep;
    impl crate::pause_gate::HostCounters for AfterSleep {
        fn mach(&self) -> u64 { 100 }
        fn raw(&self) -> u64 { 30_000_100 }
    }
    #[test]
    fn synthetic_timer_waits_for_guest_count_after_host_sleep() {
        assert!(!timer_expired(150, 10, &AfterSleep), "timer fired before guest count reached CVAL");
        assert!(timer_expired(90, 10, &AfterSleep));
    }
}

const MAX_VCPUS: usize = 16;

pub(crate) struct UsGicBridge {
    gic: Mutex<UserspaceGic>,
    num_cpus: usize,
    // Secondary entries share the owner's lifetime-protected publication.
    targets: Mutex<[Option<Arc<VcpuControl>>; MAX_VCPUS]>,
    vtimer_masked: [AtomicBool; MAX_VCPUS],
    trace_budget: AtomicU64,
    /// Ring of the most recent GIC ops (BRIDGEVM_USGIC_RING=1); dumped on
    /// SYSTEM_RESET and in stall reports. A fixed println budget always
    /// ran out before the interesting event.
    ring: Mutex<std::collections::VecDeque<String>>,
    ring_enabled: bool,
    /// WFI parking lot: vCPUs waiting for an interrupt sleep here and any
    /// GIC state change that kicks them notifies. A plain thread::sleep
    /// could not be woken by `hv_vcpus_exit` (sleep is outside
    /// `hv_vcpu_run`), which stretched guest IPI latency to the nap length
    /// and starved DXGK worker handoffs (usgic venus wedge suspect).
    wfi_lot: Mutex<u64>,
    wfi_cv: Condvar,
    /// Last CPSR+PC seen at any exit, per vCPU. HVF registers are only
    /// readable from the owning thread, so the stall report (vCPU0 thread)
    /// cannot ask vCPU1-3 directly; each records its own state here.
    last_cpsr: [AtomicU64; MAX_VCPUS],
    last_pc: [AtomicU64; MAX_VCPUS],
    last_voff: [AtomicU64; MAX_VCPUS],
    synth_fires: [AtomicU64; MAX_VCPUS],
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
        let control = Arc::new(VcpuControl::new(cpu as u64));
        control.publish_vcpu(vcpu);
        bridge.publish_target(cpu, control);
        // Apple silicon has no GICv3 CPU interface, so ID_AA64PFR0_EL1.GIC
        // reads 0 without the in-kernel GIC and guests never issue ICC_*
        // accesses (EDK2/Windows check this field before selecting the
        // sysreg interface). Advertise GIC=1; the unallocated ICC encodings
        // then trap to the Arm system-register interface implemented below.
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

pub(crate) fn register_secondary(control: Arc<VcpuControl>) {
    if let Some(bridge) = usgic() {
        bridge.publish_target(control.index as usize, control);
    }
}

/// Called by the owner guard before HVF destruction, including unwinding.
pub(crate) fn unregister_vcpu(vcpu: HvVcpuT) {
    if let Some(bridge) = usgic() {
        let mut targets = bridge.targets.lock().unwrap_or_else(|p| p.into_inner());
        for target in targets.iter_mut() {
            if target.as_ref().is_some_and(|control| {
                control.with_published_vcpu(|handle| handle == vcpu) == Some(true)
            }) {
                *target = None;
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
    access: DataAbort,
    pc: u64,
) -> bool {
    let Some(bridge) = usgic() else {
        return false;
    };
    if !UserspaceGic::owns(ipa) {
        return false;
    }
    unsafe { bridge.mmio_abort(cpu, vcpu, ipa, op, access, pc) };
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
        let line = { bridge.gic.lock().unwrap().line_asserted(cpu) };
        if !line {
            // Park on the condvar so any kick (SGI, SPI, MSI, vtimer,
            // priority change) wakes this vCPU immediately. The timeout
            // bounds the wait when a wake races the registration.
            let bit = 1u64 << cpu;
            let mut lot = bridge.wfi_lot.lock().unwrap();
            *lot |= bit;
            let (mut lot2, _) = bridge
                .wfi_cv
                .wait_timeout_while(lot, Duration::from_millis(1), |lot| *lot & bit != 0)
                .unwrap();
            *lot2 &= !bit;
        }
    }
    unsafe {
        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
    }
    true
}

/// A pause from CPU0: wake every secondary sleeping in a WFI trap (outside
/// `hv_vcpu_run`, which `hv_vcpus_exit` cannot reach) and kick the rest.
pub(crate) fn kick_secondaries() {
    if let Some(bridge) = usgic() {
        bridge.kick(u64::MAX, 0);
    }
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
        println!(
            "USGIC stall cpu{cpu}: {} cpsr={:#x} pc={:#x}",
            gic.debug_line(cpu),
            bridge.last_cpsr[cpu].load(Ordering::Relaxed),
            bridge.last_pc[cpu].load(Ordering::Relaxed)
        );
    }
    drop(gic);
    bridge.ring_dump("stall");
    unsafe {
        let mut ctl = 0u64;
        let mut cval = 0u64;
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_CNTV_CTL_EL0, &mut ctl);
        hv_vcpu_get_sys_reg(vcpu0, HV_SYS_REG_CNTV_CVAL_EL0, &mut cval);
        println!(
            "USGIC stall cpu0 vtimer: ctl={ctl:#x} cval={cval:#x} guest_now={:#x} host_masked={} synth={}",
            host_time_ticks().wrapping_sub(bridge.last_voff[0].load(Ordering::Relaxed)),
            bridge.vtimer_masked[0].load(Ordering::SeqCst),
            bridge.synth_fires[0].load(Ordering::Relaxed)
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
    bridge.reset_state();
    true
}

#[cfg(test)]
mod handle_tests {
    use super::*;

    #[test]
    fn usgic_kick_holds_publication_until_provider_returns() {
        let bridge = Arc::new(UsGicBridge::new(2));
        let control = Arc::new(VcpuControl::new(1));
        control.publish_vcpu(0x5678);
        bridge.publish_target(1, Arc::clone(&control));
        let (entered_tx, entered_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        let kicker = {
            let bridge = Arc::clone(&bridge);
            std::thread::spawn(move || bridge.with_kick_targets(2, 0, |handle| {
                assert_eq!(handle, 0x5678);
                entered_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            }))
        };
        entered_rx.recv().unwrap();
        let retire = std::thread::spawn(move || {
            control.withdraw_vcpu(0x5678);
            done_tx.send(()).unwrap();
        });
        let blocked = done_rx.recv_timeout(Duration::from_millis(25)).is_err();
        release_tx.send(()).unwrap();
        kicker.join().unwrap();
        retire.join().unwrap();
        assert!(blocked, "handle retired while kick provider was using it");
        bridge.reset_state();
        bridge.with_kick_targets(2, 0, |_| panic!("retired handle reused"));
    }

    #[test]
    fn usgic_kick_does_not_use_withdrawn_secondary_handle() {
        let bridge = UsGicBridge::new(2);
        let control = Arc::new(VcpuControl::new(1));
        control.publish_vcpu(0x1234);
        bridge.publish_target(1, Arc::clone(&control));
        let mut calls = Vec::new();
        bridge.with_kick_targets(2, 0, |handle| calls.push(handle));
        assert_eq!(calls, vec![0x1234]);
        control.withdraw_vcpu(0x1234);
        calls.clear();
        bridge.reset_state();
        bridge.with_kick_targets(2, 0, |handle| calls.push(handle));
        assert!(calls.is_empty(), "kick used a retired secondary handle: {calls:?}");
    }
}

impl UsGicBridge {
    fn new(num_cpus: usize) -> Self {
        Self {
            gic: Mutex::new(UserspaceGic::new(num_cpus)),
            num_cpus,
            targets: Mutex::new(std::array::from_fn(|_| None)),
            vtimer_masked: [const { AtomicBool::new(false) }; MAX_VCPUS],
            trace_budget: AtomicU64::new(env_u64("BRIDGEVM_USGIC_TRACE", 0)),
            ring: Mutex::new(std::collections::VecDeque::with_capacity(256)),
            ring_enabled: env_flag("BRIDGEVM_USGIC_RING"),
            wfi_lot: Mutex::new(0),
            wfi_cv: Condvar::new(),
            last_cpsr: std::array::from_fn(|_| AtomicU64::new(0)),
            last_pc: std::array::from_fn(|_| AtomicU64::new(0)),
            last_voff: std::array::from_fn(|_| AtomicU64::new(0)),
            synth_fires: std::array::from_fn(|_| AtomicU64::new(0)),
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
        // Wake any WFI-parked target first: they are not inside
        // hv_vcpu_run, so hv_vcpus_exit alone cannot reach them.
        {
            let mut lot = self.wfi_lot.lock().unwrap();
            if *lot & mask != 0 {
                *lot &= !mask;
                self.wfi_cv.notify_all();
            }
        }
        self.with_kick_targets(mask, self_cpu, |handle| unsafe {
            hv_vcpus_exit(&handle, 1);
        });
    }

    fn reset_state(&self) {
        *self.gic.lock().unwrap() = UserspaceGic::new(self.num_cpus);
        // HVF timer-mask APIs are owner-thread-only. CPU0's boot reset and
        // each secondary's CPU_ON reset unmask their own live timer.
        for masked in &self.vtimer_masked {
            masked.store(false, Ordering::SeqCst);
        }
    }

    fn publish_target(&self, cpu: usize, control: Arc<VcpuControl>) {
        self.targets.lock().unwrap()[cpu] = Some(control);
    }

    fn with_kick_targets(&self, mask: u64, self_cpu: usize, mut action: impl FnMut(HvVcpuT)) {
        let targets = self.targets.lock().unwrap_or_else(|p| p.into_inner());
        for (cpu, target) in targets.iter().enumerate().take(self.num_cpus) {
            if cpu == self_cpu || mask & (1u64 << cpu) == 0 {
                continue;
            }
            if let Some(control) = target {
                control.with_published_vcpu(&mut action);
            }
        }
    }

    unsafe fn pre_run(&self, cpu: usize, vcpu: HvVcpuT) {
        // Synthesize the vtimer fire that Apple's kernel loses under load.
        // The r3d-1-072906 park: CVAL 0x29eafc5416d3 < now, ENABLE=1,
        // IMASK=0, host unmasked -- and 258k recovery calls (25k mask
        // pulses, 33k re-arms) never coaxed EXIT_VTIMER out of HVF. The
        // fire GENERATION path still lives in Apple's kernel even with the
        // userspace GIC, so stop depending on it: on every entry, if the
        // guest's own CNTV state says "expired and unmasked", latch the
        // PPI ourselves and mask the host timer exactly as EXIT_VTIMER
        // would have.
        {
            let mut cpsr = 0u64;
            let mut pc = 0u64;
            hv_vcpu_get_reg(vcpu, HV_REG_CPSR, &mut cpsr);
            hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
            self.last_cpsr[cpu].store(cpsr, Ordering::Relaxed);
            self.last_pc[cpu].store(pc, Ordering::Relaxed);
        }
        if !self.vtimer_masked[cpu].load(Ordering::SeqCst) {
            let mut ctl = 0u64;
            hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut ctl);
            // ENABLE=1, IMASK=0: ISTATUS (bit2) may lag, so compare CVAL.
            if ctl & 0b11 == 0b01 {
                let expired = if ctl & 0b100 != 0 {
                    true
                } else {
                    let mut cval = 0u64;
                    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cval);
                    let mut voff = 0u64;
                    hv_vcpu_get_vtimer_offset(vcpu, &mut voff);
                    self.last_voff[cpu].store(voff, Ordering::Relaxed);
                    timer_expired(cval, voff, &crate::pause_gate::Host)
                };
                if expired {
                    hv_vcpu_set_vtimer_mask(vcpu, true);
                    self.vtimer_masked[cpu].store(true, Ordering::SeqCst);
                    self.synth_fires[cpu].fetch_add(1, Ordering::Relaxed);
                    let kicks = self.gic.lock().unwrap().set_vtimer_ppi(cpu, true);
                    self.ring_push(format!("c{cpu} vtimer-synth k={kicks:#x}"));
                    self.kick(kicks, cpu);
                }
            }
        }
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
        access: DataAbort,
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
        write_mmio_read(vcpu, access, result.value);
        self.kick(result.kick_mask, cpu);
        hv_vcpu_set_reg(vcpu, HV_REG_PC, pc + 4);
    }
}
