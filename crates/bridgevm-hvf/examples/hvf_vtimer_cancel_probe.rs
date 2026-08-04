//! Bare-metal microprobe for the Apple HVF vtimer/cancellation race.
//!
//! The A1 boot stall leaves a Windows guest parked in `WFI` with `CNTV_CVAL`
//! already in the past and no further `EXIT_VTIMER` ever arriving. The measured
//! correlation is with surplus `hv_vcpus_exit` cancellations: HVF auto-masks
//! the vtimer when it fires, and if a cancellation wins the race against an
//! in-flight fire, the exit surfaces as `EXIT_CANCELED`, the unmask site never
//! runs, and the wake is lost.
//!
//! Reproducing that through a Windows boot costs roughly 20 minutes per
//! sample. This probe reproduces the same host-side condition against a ~40
//! instruction EL1 guest in seconds, so a candidate fix can be judged before
//! spending a boot gate on it.
//!
//! What it does NOT prove: that the Windows stall has no additional cause.
//! It exercises the timer/cancellation path only. A pass here is a
//! precondition for an A1 campaign, never a substitute for one.
//!
//! Build, sign (needs `com.apple.security.hypervisor`), run:
//!   cargo build -p bridgevm-hvf --example hvf_vtimer_cancel_probe
//!   codesign --sign - --entitlements hv.entitlements --force \
//!     target/debug/examples/hvf_vtimer_cancel_probe
//!   target/debug/examples/hvf_vtimer_cancel_probe --iterations 10000

use std::alloc::{alloc_zeroed, Layout};
use std::os::raw::c_void;
use std::ptr::null_mut;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[path = "hvf_vtimer_cancel_probe/guest_code.rs"]
mod guest_code;
#[path = "hvf_vtimer_cancel_probe/options.rs"]
mod options;
#[path = "hvf_vtimer_cancel_probe/receipt.rs"]
mod receipt;

use guest_code::{
    ARM_DELTA_OFFSET, CONTROL_PAGE_OFFSET, FIRED_FLAG_OFFSET, GUEST_BASE, GUEST_HANDLER,
    GUEST_MAIN, VECTOR_IRQ_OFFSET, VECTOR_TABLE_OFFSET, WAKE_COUNTER_OFFSET,
};
use options::Options;
use receipt::{Counters, Outcome};

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
    fn hv_vcpu_get_reg(vcpu: HvVcpuT, reg: u32, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_sys_reg(vcpu: HvVcpuT, reg: u16, value: u64) -> HvReturn;
    fn hv_vcpu_get_sys_reg(vcpu: HvVcpuT, reg: u16, value: *mut u64) -> HvReturn;
    fn hv_vcpu_set_vtimer_mask(vcpu: HvVcpuT, vtimer_is_masked: bool) -> HvReturn;
    fn hv_vcpu_get_vtimer_mask(vcpu: HvVcpuT, vtimer_is_masked: *mut bool) -> HvReturn;
    fn hv_vcpu_get_vtimer_offset(vcpu: HvVcpuT, offset: *mut u64) -> HvReturn;
    fn hv_gic_config_create() -> HvGicConfig;
    fn hv_gic_config_set_distributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_config_set_redistributor_base(config: HvGicConfig, base: u64) -> HvReturn;
    fn hv_gic_create(config: HvGicConfig) -> HvReturn;
}

const HV_REG_PC: u32 = 31;
const HV_REG_CPSR: u32 = 34;
const HV_SYS_REG_MPIDR_EL1: u16 = 0xc005;
const HV_SYS_REG_CNTV_CTL_EL0: u16 = 0xdf19;
const HV_SYS_REG_CNTV_CVAL_EL0: u16 = 0xdf1a;
const HV_MEMORY_READ: u64 = 1;
const HV_MEMORY_WRITE: u64 = 2;
const HV_MEMORY_EXEC: u64 = 4;
const EXIT_CANCELED: u32 = 0;
const EXIT_EXCEPTION: u32 = 1;
const EXIT_VTIMER: u32 = 2;
const MAPPING_SIZE: usize = 0x1_0000;

fn host_cntvct() -> u64 {
    let value: u64;
    // SAFETY: CNTVCT_EL0 is an unprivileged architectural counter read with no
    // side effects.
    unsafe { std::arch::asm!("mrs {}, cntvct_el0", out(reg) value, options(nomem, nostack)) };
    value
}

/// The recovery under test, kept byte-for-byte equivalent in intent to
/// `probe_runtime/vtimer_recovery.rs`. Returns true when it rewrote a deadline
/// that had already passed, which is the swallowed-fire case.
///
/// # Safety
/// `vcpu` must be a live vCPU owned by the calling thread.
unsafe fn recover_swallowed_vtimer_fire(vcpu: HvVcpuT) -> bool {
    hv_vcpu_set_vtimer_mask(vcpu, false);
    let mut cntv_ctl = 0u64;
    let mut cntv_cval = 0u64;
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut cntv_ctl);
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cntv_cval);
    // ENABLE=1 and IMASK=0: the guest is waiting on this timer.
    if cntv_ctl & 0b11 != 0b01 {
        return false;
    }
    let mut voff = 0u64;
    hv_vcpu_get_vtimer_offset(vcpu, &mut voff);
    let guest_now = host_cntvct().wrapping_sub(voff);
    if cntv_cval > guest_now {
        return false;
    }
    hv_vcpu_set_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, guest_now);
    true
}

/// True when the guest is parked at its `WFI` with the vtimer masked and the
/// armed deadline already in the past: it can never be woken by that timer
/// again without host intervention.
///
/// The `WFI` check is load-bearing. Without it this also matches the guest
/// mid-flight between servicing an interrupt and re-arming, where the state is
/// benign and self-correcting -- a state that occurs tens of thousands of
/// times per run and would drown the real signal.
///
/// # Safety
/// `vcpu` must be a live vCPU owned by the calling thread.
unsafe fn masked_with_expired_deadline(vcpu: HvVcpuT) -> bool {
    let mut pc = 0u64;
    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
    if pc != guest_code::wfi_address() {
        return false;
    }
    let mut masked = false;
    hv_vcpu_get_vtimer_mask(vcpu, &mut masked);
    if !masked {
        return false;
    }
    let mut cntv_ctl = 0u64;
    let mut cntv_cval = 0u64;
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut cntv_ctl);
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cntv_cval);
    if cntv_ctl & 0b11 != 0b01 {
        return false;
    }
    let mut voff = 0u64;
    hv_vcpu_get_vtimer_offset(vcpu, &mut voff);
    cntv_cval <= host_cntvct().wrapping_sub(voff)
}

fn main() {
    let options = match Options::parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(message) => {
            eprintln!("{message}");
            std::process::exit(2);
        }
    };

    // SAFETY: single-threaded VM setup; every pointer below is either freshly
    // allocated here or returned by the framework call that precedes it.
    let (counters, outcome, elapsed) = unsafe { run(&options) };

    let json = receipt::to_json(&counters, outcome, elapsed.as_millis());
    println!("{json}");
    if let Some(path) = options.receipt_path.as_ref() {
        if let Some(parent) = std::path::Path::new(path).parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if let Err(error) = std::fs::write(path, format!("{json}\n")) {
            eprintln!("failed to write receipt to {path}: {error}");
            std::process::exit(1);
        }
    }

    let failures = receipt::failures(&counters, outcome);
    if failures.is_empty() {
        println!(
            "PASS: {} iterations, every timer wake delivered",
            counters.iterations
        );
        return;
    }
    for failure in &failures {
        eprintln!("FAIL: {failure}");
    }
    std::process::exit(1);
}

/// # Safety
/// Creates and destroys a VM for the calling process; must be called once.
unsafe fn run(options: &Options) -> (Counters, Outcome, Duration) {
    assert_eq!(hv_vm_create(null_mut()), 0, "hv_vm_create");
    let gic = hv_gic_config_create();
    hv_gic_config_set_distributor_base(gic, 0x0800_0000);
    hv_gic_config_set_redistributor_base(gic, 0x080a_0000);
    assert_eq!(hv_gic_create(gic), 0, "hv_gic_create");

    let layout = Layout::from_size_align(MAPPING_SIZE, 0x1_0000).unwrap();
    let mem = alloc_zeroed(layout);
    assert!(!mem.is_null(), "guest memory allocation");
    for (i, word) in GUEST_MAIN.iter().enumerate() {
        std::ptr::copy_nonoverlapping(word.to_le_bytes().as_ptr(), mem.add(i * 4), 4);
    }
    for (i, word) in GUEST_HANDLER.iter().enumerate() {
        let at = VECTOR_TABLE_OFFSET + VECTOR_IRQ_OFFSET + i * 4;
        std::ptr::copy_nonoverlapping(word.to_le_bytes().as_ptr(), mem.add(at), 4);
    }

    let control = mem.add(CONTROL_PAGE_OFFSET);
    let wake_counter = control.add(WAKE_COUNTER_OFFSET) as *const u64;
    std::ptr::write_volatile(control.add(ARM_DELTA_OFFSET) as *mut u64, options.arm_ticks);

    assert_eq!(
        hv_vm_map(
            mem as *mut c_void,
            GUEST_BASE,
            MAPPING_SIZE,
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

    // The racer fires cancellations continuously, with no coordination with
    // the guest's timer, so cancels land uniformly across the fire window.
    // This is the stimulus the A1 boot path produces incidentally through its
    // ramfb/vblank/agent wakes.
    let stop = Arc::new(AtomicBool::new(false));
    let paused = Arc::new(AtomicBool::new(false));
    let cancels = Arc::new(AtomicU64::new(0));
    let racer = {
        let stop = Arc::clone(&stop);
        let paused = Arc::clone(&paused);
        let cancels = Arc::clone(&cancels);
        let interval = options.cancel_interval;
        let target = vcpu;
        std::thread::spawn(move || {
            while !stop.load(Ordering::Relaxed) {
                if !paused.load(Ordering::Relaxed) {
                    // SAFETY: hv_vcpus_exit is the one vCPU call documented as
                    // callable from another thread.
                    unsafe { hv_vcpus_exit(&target, 1) };
                    cancels.fetch_add(1, Ordering::Relaxed);
                }
                if !interval.is_zero() {
                    std::thread::sleep(interval);
                }
            }
        })
    };

    let started = Instant::now();
    let mut counters = Counters {
        iterations: options.iterations,
        ..Counters::default()
    };
    let mut outcome = Outcome::Completed;
    let mut last_progress = Instant::now();
    let mut last_wakes = 0u64;
    let mut quiesced = false;
    let mut quiesce_verdict: Option<bool> = None;

    loop {
        let status = hv_vcpu_run(vcpu);
        assert_eq!(status, 0, "hv_vcpu_run");
        let reason = (*exit).reason;

        match reason {
            EXIT_CANCELED => {
                counters.canceled_exits += 1;
                // Every cancel here is surplus: this probe has no host work
                // pending behind one, unlike the boot path's wake sources.
                counters.surplus_canceled += 1;
                if masked_with_expired_deadline(vcpu) {
                    counters.masked_past_deadline += 1;
                    if options.quiesce_probe && !quiesced {
                        quiesced = true;
                        quiesce_verdict =
                            Some(quiesce_experiment(vcpu, exit, &paused, wake_counter));
                    }
                }
                if options.recover && recover_swallowed_vtimer_fire(vcpu) {
                    counters.recoveries += 1;
                }
            }
            EXIT_VTIMER => {
                counters.vtimer_exits += 1;
                hv_vcpu_set_vtimer_mask(vcpu, true);
            }
            EXIT_EXCEPTION => {
                let esr = (*exit).exception.syndrome;
                let mut pc = 0u64;
                hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
                eprintln!(
                    "guest exception EC {:#x} ESR {esr:#x} PC {pc:#x}",
                    (esr >> 26) & 0x3f
                );
                outcome = Outcome::Faulted;
                break;
            }
            other => {
                eprintln!("unexpected exit reason {other}");
                outcome = Outcome::Faulted;
                break;
            }
        }

        let wakes = std::ptr::read_volatile(wake_counter);
        counters.timer_wakes = wakes.min(options.iterations);
        if wakes >= options.iterations {
            break;
        }
        if wakes != last_wakes {
            last_wakes = wakes;
            last_progress = Instant::now();
        } else if last_progress.elapsed() >= options.stall_timeout {
            // No new wake within the deadline: this is the condition the probe
            // exists to catch. Report the guest's parked state before stopping.
            report_stall(vcpu, wakes);
            outcome = Outcome::Stalled;
            break;
        }
    }

    let elapsed = started.elapsed();
    stop.store(true, Ordering::Relaxed);
    let _ = racer.join();
    if let Some(woke) = quiesce_verdict {
        if woke {
            println!(
                "QUIESCE: with all cancellation stopped the swallowed fire was still \
                 delivered -- masked+past-CVAL is transient here, not a lost wake"
            );
        } else {
            println!(
                "QUIESCE: with all cancellation stopped the wake never arrived -- \
                 the fire was genuinely lost and only host intervention can recover it"
            );
        }
    }

    hv_vcpu_destroy(vcpu);
    hv_vm_destroy();
    (counters, outcome, elapsed)
}

/// Snapshot the stalled guest from the thread that owns the vCPU.
///
/// Two samples are taken because "stalled" has two very different causes that
/// look alike from the outside. If `CVAL` is unchanged between them the guest
/// is genuinely parked on a deadline that will never be delivered, which is
/// the A1 condition. If `CVAL` moves, the guest is still executing and
/// re-arming, so the run is starved rather than stalled -- normally because
/// the cancellation rate leaves the vCPU no time to make progress.
///
/// # Safety
/// `vcpu` must be live and owned by the calling thread.
unsafe fn report_stall(vcpu: HvVcpuT, wakes: u64) {
    let first = sample_timer(vcpu);
    std::thread::sleep(Duration::from_millis(200));
    let second = sample_timer(vcpu);
    let verdict = if first.cval == second.cval {
        "parked (CVAL unchanged: the deadline will never be delivered)"
    } else {
        "starved (CVAL moved: the guest is still executing and re-arming)"
    };
    eprintln!(
        "STALL after {wakes} wakes: {verdict}\n  \
         PC={:#x} masked={} CNTV_CTL={:#x} CVAL={:#x} guest_now={:#x} overdue_ticks={}\n  \
         after 200ms: PC={:#x} masked={} CVAL={:#x} guest_now={:#x} guest_ticks_elapsed={}",
        first.pc,
        first.masked,
        first.ctl,
        first.cval,
        first.guest_now,
        first.guest_now.saturating_sub(first.cval),
        second.pc,
        second.masked,
        second.cval,
        second.guest_now,
        second.guest_now.wrapping_sub(first.guest_now),
    );
}

/// With the vtimer masked and its deadline past, stop every cancellation and
/// give the guest a quiet window. Returns whether the wake still arrived.
///
/// This separates "HVF swallowed the fire permanently" from "the counter saw a
/// transient state that resolves itself". The counters alone cannot tell those
/// apart, and the difference decides whether recovery is load-bearing.
///
/// # Safety
/// `vcpu` must be live and owned by the calling thread, `exit` must be its
/// exit structure, and `wake_counter` must point into the mapped control page.
unsafe fn quiesce_experiment(
    vcpu: HvVcpuT,
    exit: *mut HvVcpuExit,
    paused: &Arc<AtomicBool>,
    wake_counter: *const u64,
) -> bool {
    let before = std::ptr::read_volatile(wake_counter);
    paused.store(true, Ordering::Relaxed);
    // The guest only leaves its WFI loop when the handler sets this flag, so a
    // wake and the flag must agree; reading it keeps the two in step.
    let fired_flag = wake_counter
        .cast::<u8>()
        .add(FIRED_FLAG_OFFSET)
        .cast::<u64>();
    // Let any in-flight cancellation drain, then drop the stale exits it left.
    std::thread::sleep(Duration::from_millis(20));
    let drain_until = Instant::now() + Duration::from_millis(50);
    while Instant::now() < drain_until {
        let watchdog = spawn_watchdog(vcpu, Duration::from_millis(10));
        hv_vcpu_run(vcpu);
        let _ = watchdog.join();
        if std::ptr::read_volatile(wake_counter) != before {
            debug_assert_ne!(std::ptr::read_volatile(fired_flag), 0);
            paused.store(false, Ordering::Relaxed);
            return true;
        }
    }
    // Nothing is cancelling now. Keep running until either the wake arrives or
    // the deadline expires; a lingering CANCELED exit is a stale cancel from
    // before the pause, not an answer, so it must not end the experiment.
    let quiet_started = Instant::now();
    let deadline = Duration::from_millis(500);
    let mut quiet_reason = u32::MAX;
    let mut woke = false;
    while quiet_started.elapsed() < deadline {
        let watchdog = spawn_watchdog(vcpu, deadline - quiet_started.elapsed());
        hv_vcpu_run(vcpu);
        quiet_reason = (*exit).reason;
        let _ = watchdog.join();
        if std::ptr::read_volatile(wake_counter) != before {
            woke = true;
            break;
        }
    }
    let quiet_elapsed = quiet_started.elapsed();
    let sample = sample_timer(vcpu);
    eprintln!(
        "  quiesce detail: waited {:?} in a fully quiet window, woke={woke}, \
         exit_reason={quiet_reason}, PC={:#x} masked={} CNTV_CTL={:#x} overdue_ticks={}",
        quiet_elapsed,
        sample.pc,
        sample.masked,
        sample.ctl,
        sample.guest_now.saturating_sub(sample.cval),
    );
    paused.store(false, Ordering::Relaxed);
    woke
}

fn spawn_watchdog(vcpu: HvVcpuT, after: Duration) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        std::thread::sleep(after);
        // SAFETY: hv_vcpus_exit is callable from another thread.
        unsafe { hv_vcpus_exit(&vcpu, 1) };
    })
}

struct TimerSample {
    pc: u64,
    masked: bool,
    ctl: u64,
    cval: u64,
    guest_now: u64,
}

/// # Safety
/// `vcpu` must be live and owned by the calling thread.
unsafe fn sample_timer(vcpu: HvVcpuT) -> TimerSample {
    let mut masked = false;
    let mut ctl = 0u64;
    let mut cval = 0u64;
    let mut voff = 0u64;
    let mut pc = 0u64;
    hv_vcpu_get_vtimer_mask(vcpu, &mut masked);
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CTL_EL0, &mut ctl);
    hv_vcpu_get_sys_reg(vcpu, HV_SYS_REG_CNTV_CVAL_EL0, &mut cval);
    hv_vcpu_get_vtimer_offset(vcpu, &mut voff);
    hv_vcpu_get_reg(vcpu, HV_REG_PC, &mut pc);
    TimerSample {
        pc,
        masked,
        ctl,
        cval,
        guest_now: host_cntvct().wrapping_sub(voff),
    }
}
