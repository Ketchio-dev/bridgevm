//! Shared vCPU lifecycle, automation, and pre-run coordination.

use crate::*;

#[path = "vcpu_coordination/drain_gate.rs"]
mod drain_gate;
pub(crate) use drain_gate::PreRunDrainGate;
#[path = "vcpu_coordination/run_failure.rs"]
mod run_failure;
pub(crate) use run_failure::run_hvf_vcpu_once;
#[path = "vcpu_coordination/secondary_stop.rs"]
mod secondary_stop;
pub(crate) use secondary_stop::SecondaryUnexpectedStop;
#[path = "vcpu_coordination/shutdown.rs"]
mod shutdown;
#[path = "vcpu_coordination/stop_outcome.rs"]
mod stop_outcome;

pub(crate) struct VcpuControl {
    pub(crate) state: Mutex<PsciState>,
    pub(crate) condvar: Condvar,
    pub(crate) entry: AtomicU64,
    pub(crate) context: AtomicU64,
    /// Live HVF handle published by the secondary thread. The mutex is also
    /// the handle-lifetime barrier: shutdown holds it while requesting an
    /// exit, and the owner clears it before dropping `HvVcpuGuard`.
    pub(crate) vcpu: Mutex<Option<HvVcpuT>>,
    pub(crate) created: AtomicBool,
    /// One owner-thread snapshot, published before the live handle is
    /// withdrawn. Guest RAM is not interpreted until all threads are joined.
    pub(crate) final_state: Mutex<Option<VcpuFinalState>>,
    pub(crate) exits: AtomicU64,
    pub(crate) run_error: AtomicBool,
    pub(crate) mpidr: u64,
    pub(crate) index: u64,
}

impl VcpuControl {
    pub(crate) fn new(index: u64) -> Self {
        Self {
            state: Mutex::new(PsciState::Off),
            condvar: Condvar::new(),
            entry: AtomicU64::new(0),
            context: AtomicU64::new(0),
            vcpu: Mutex::new(None),
            created: AtomicBool::new(false),
            final_state: Mutex::new(None),
            exits: AtomicU64::new(0),
            run_error: AtomicBool::new(false),
            mpidr: 0x8000_0000 | machine::cpu_mpidr(index),
            index,
        }
    }
    pub(crate) fn publish_vcpu(&self, vcpu: HvVcpuT) {
        let mut published = self.vcpu.lock().expect("secondary vCPU handle mutex");
        self.created.store(true, Ordering::Release);
        assert!(
            published.replace(vcpu).is_none(),
            "secondary vCPU{} handle published twice",
            self.index
        );
    }
    pub(crate) fn withdraw_vcpu(&self, vcpu: HvVcpuT) {
        let mut published = self.vcpu.lock().expect("secondary vCPU handle mutex");
        assert_eq!(
            *published,
            Some(vcpu),
            "secondary vCPU{} handle withdrawal mismatch",
            self.index
        );
        *published = None;
    }
    pub(crate) fn with_published_vcpu<R>(&self, action: impl FnOnce(HvVcpuT) -> R) -> Option<R> {
        // Cleanup must still join other owners if an owner poisoned this lock.
        let published = self
            .vcpu
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let vcpu = (*published)?;
        Some(action(vcpu))
    }
    pub(crate) fn request_exit_if_published(&self) {
        // Keep the publication lock held across the synchronous request. The
        // owner cannot withdraw and destroy this handle until the call returns.
        let _ = self.with_published_vcpu(|vcpu| unsafe { hv_vcpus_exit(&vcpu, 1) });
    }
}

pub(crate) struct SecondaryVcpuSet {
    pub(crate) shutdown: Arc<AtomicBool>,
    pub(crate) terminal: Arc<SecondaryTerminalSignal>,
    pub(crate) controls: Vec<Arc<VcpuControl>>,
    pub(crate) handles: Vec<JoinHandle<()>>,
}

/// Carries a terminal PSCI request made by any secondary vCPU back to the
/// primary run loop. PSCI permits SYSTEM_OFF/SYSTEM_RESET on any online CPU;
/// terminating only the calling secondary leaves CPU0 and the VM alive.
pub(crate) struct SecondaryTerminalSignal {
    pub(crate) function: AtomicU64,
}

impl SecondaryTerminalSignal {
    pub(crate) const fn new() -> Self {
        Self {
            function: AtomicU64::new(0),
        }
    }
    pub(crate) fn record(&self, function: u64) -> bool {
        let function = function & 0xffff_ffff;
        debug_assert!(psci_terminal_action(function).is_some());
        self.function
            .compare_exchange(0, function, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
    pub(crate) fn action(&self) -> Option<PsciTerminalAction> {
        psci_terminal_action(self.function.load(Ordering::SeqCst))
    }
}

pub(crate) struct AutomationGate {
    pub(crate) always_check: bool,
    pub(crate) serial_dirty: bool,
}

impl AutomationGate {
    pub(crate) const fn new(always_check: bool) -> Self {
        Self {
            always_check,
            serial_dirty: true,
        }
    }
    pub(crate) fn mark_serial_output_dirty(&mut self) {
        self.serial_dirty = true;
    }
    pub(crate) fn should_check(&self, automation_tick_canceled: bool) -> bool {
        self.always_check || automation_tick_canceled || self.serial_dirty
    }
    pub(crate) fn note_checked(&mut self) {
        if !self.always_check {
            self.serial_dirty = false;
        }
    }
}

#[cfg(test)]
mod automation_gate_tests {
    use super::*;

    #[test]
    fn default_gate_runs_initially_then_only_after_serial_or_host_tick() {
        let mut gate = AutomationGate::new(false);

        assert!(gate.should_check(false));
        gate.note_checked();
        assert!(!gate.should_check(false));
        assert!(gate.should_check(true));
        gate.note_checked();
        assert!(!gate.should_check(false));

        gate.mark_serial_output_dirty();
        assert!(gate.should_check(false));
    }

    #[test]
    fn opt_in_automation_preserves_always_check_behavior() {
        let mut gate = AutomationGate::new(true);

        assert!(gate.should_check(false));
        gate.note_checked();
        assert!(gate.should_check(false));
    }

    #[test]
    fn periodic_timer_gate_skips_ordinary_cpu0_exits_between_host_wakes() {
        let mut gate = AutomationGate::new(false);

        assert!(gate.should_check(false));
        gate.note_checked();
        for _ in 0..1_000 {
            assert!(!gate.should_check(false));
        }

        assert!(gate.should_check(true));
        gate.note_checked();
        assert!(!gate.should_check(false));
    }
}

pub(crate) struct SecondaryVcpuSpawnConfig {
    pub(crate) cpu_count: u64,
    pub(crate) primary_vcpu: HvVcpuT,
    pub(crate) ram_base: usize,
    pub(crate) ram_size: usize,
    pub(crate) platform: Arc<Mutex<VirtPlatform>>,
    pub(crate) drain_trace: DrainTrace,
    pub(crate) pre_run_drain_gate: Arc<PreRunDrainGate>,
    pub(crate) smp_trace: Option<Arc<SmpTrace>>,
    pub(crate) max_exits: u64,
    pub(crate) generation: u64,
}

pub(crate) struct SecondaryVcpuThreadContext {
    pub(crate) shutdown: Arc<AtomicBool>,
    pub(crate) terminal: Arc<SecondaryTerminalSignal>,
    pub(crate) primary_vcpu: HvVcpuT,
    pub(crate) ram_base: usize,
    pub(crate) ram_size: usize,
    pub(crate) platform: Arc<Mutex<VirtPlatform>>,
    pub(crate) drain_trace: DrainTrace,
    pub(crate) pre_run_drain_gate: Arc<PreRunDrainGate>,
    pub(crate) controls: Vec<Arc<VcpuControl>>,
    pub(crate) smp_trace: Option<Arc<SmpTrace>>,
    pub(crate) max_exits: u64,
    pub(crate) generation: u64,
}

impl SecondaryVcpuSet {
    pub(crate) fn spawn(config: SecondaryVcpuSpawnConfig) -> Self {
        let SecondaryVcpuSpawnConfig {
            cpu_count,
            primary_vcpu,
            ram_base,
            ram_size,
            platform,
            drain_trace,
            pre_run_drain_gate,
            smp_trace,
            max_exits,
            generation,
        } = config;
        // Own each handle as soon as it is spawned: a later spawn failure
        // unwinds through this set and joins the already-started owners.
        let mut set = Self {
            shutdown: Arc::new(AtomicBool::new(false)),
            terminal: Arc::new(SecondaryTerminalSignal::new()),
            controls: (1..cpu_count)
                .map(|index| Arc::new(VcpuControl::new(index)))
                .collect(),
            handles: Vec::new(),
        };
        for control in &set.controls {
            let control = Arc::clone(control);
            let controls_for_thread = set.controls.clone();
            let shutdown = Arc::clone(&set.shutdown);
            let terminal = Arc::clone(&set.terminal);
            let platform = Arc::clone(&platform);
            let pre_run_drain_gate = Arc::clone(&pre_run_drain_gate);
            let smp_trace = smp_trace.clone();
            let handle = thread::Builder::new()
                .name(format!("bridgevm-hvf-vcpu{}", control.index))
                .spawn(move || {
                    raise_vcpu_qos();
                    secondary_vcpu_thread(
                        control,
                        SecondaryVcpuThreadContext {
                            shutdown,
                            terminal,
                            primary_vcpu,
                            ram_base,
                            ram_size,
                            platform,
                            drain_trace,
                            pre_run_drain_gate,
                            controls: controls_for_thread,
                            smp_trace,
                            max_exits,
                            generation,
                        },
                    )
                })
                .expect("spawn secondary vCPU thread");
            set.handles.push(handle);
        }
        set
    }
    pub(crate) fn terminal_action(&self) -> Option<PsciTerminalAction> {
        self.terminal.action()
    }
}

#[cfg(test)]
#[path = "vcpu_coordination_tests.rs"]
mod tests;

/// User-initiated QoS for a vCPU thread: the guest's own work, which the user
/// is waiting on. At the default QoS macOS may run it on an efficiency core
/// while the Mac is busy; this keeps it on performance cores and still below
/// the Mac's own interface (user-interactive). A failure leaves the default.
pub(crate) fn raise_vcpu_qos() {
    use libc::qos_class_t::QOS_CLASS_USER_INITIATED;
    // SAFETY: sets only the calling thread's own QoS class; no pointers.
    let status = unsafe { libc::pthread_set_qos_class_self_np(QOS_CLASS_USER_INITIATED, 0) };
    if status != 0 {
        eprintln!("vCPU thread: QoS not raised: {status}");
    }
}

#[cfg(test)]
mod qos_tests {
    #[test]
    fn a_vcpu_thread_runs_at_user_initiated_qos() {
        let qos = std::thread::spawn(|| {
            super::raise_vcpu_qos();
            let mut class = libc::qos_class_t::QOS_CLASS_UNSPECIFIED;
            let mut relative = 0;
            // SAFETY: reads the calling thread's own QoS into two locals.
            let status = unsafe {
                libc::pthread_get_qos_class_np(libc::pthread_self(), &mut class, &mut relative)
            };
            assert_eq!(status, 0);
            class as u32
        })
        .join()
        .unwrap();
        assert_eq!(qos, libc::qos_class_t::QOS_CLASS_USER_INITIATED as u32);
    }
}
