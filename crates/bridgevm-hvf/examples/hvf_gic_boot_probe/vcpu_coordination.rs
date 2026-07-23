//! Shared vCPU lifecycle, automation, and pre-run coordination.

use crate::*;

pub(crate) struct VcpuControl {
    pub(crate) state: Mutex<PsciState>,
    pub(crate) condvar: Condvar,
    pub(crate) entry: AtomicU64,
    pub(crate) context: AtomicU64,
    /// Live HVF handle published by the secondary thread. The mutex is also
    /// the handle-lifetime barrier: shutdown holds it while requesting an
    /// exit, and the owner clears it before dropping `HvVcpuGuard`.
    pub(crate) vcpu: Mutex<Option<HvVcpuT>>,
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
            exits: AtomicU64::new(0),
            run_error: AtomicBool::new(false),
            mpidr: 0x8000_0000 | machine::cpu_mpidr(index),
            index,
        }
    }
    pub(crate) fn notify_shutdown(&self) {
        self.condvar.notify_all();
    }
    pub(crate) fn publish_vcpu(&self, vcpu: HvVcpuT) {
        let mut published = self.vcpu.lock().expect("secondary vCPU handle mutex");
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
        let published = self.vcpu.lock().expect("secondary vCPU handle mutex");
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

pub(crate) struct PreRunDrainGate {
    pub(crate) enabled: bool,
    pub(crate) secondary_pending: AtomicBool,
}

impl PreRunDrainGate {
    pub(crate) fn from_env() -> Self {
        Self::new(env_flag_default("BRIDGEVM_DRAIN_GATE", true))
    }
    pub(crate) const fn new(enabled: bool) -> Self {
        Self {
            enabled,
            secondary_pending: AtomicBool::new(true),
        }
    }
    pub(crate) fn should_drain_secondary_pre_run(&self) -> bool {
        if !self.enabled {
            return true;
        }
        self.secondary_pending.swap(false, Ordering::AcqRel)
    }
    pub(crate) fn mark_secondary_pending(&self) {
        if self.enabled {
            self.secondary_pending.store(true, Ordering::Release);
        }
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

#[cfg(test)]
mod pre_run_drain_gate_tests {
    use super::*;

    #[test]
    fn enabled_gate_allows_initial_secondary_drain_once() {
        let gate = PreRunDrainGate::new(true);

        assert!(gate.should_drain_secondary_pre_run());
        assert!(!gate.should_drain_secondary_pre_run());
    }

    #[test]
    fn enabled_gate_allows_drain_after_pending_marker() {
        let gate = PreRunDrainGate::new(true);

        assert!(gate.should_drain_secondary_pre_run());
        assert!(!gate.should_drain_secondary_pre_run());
        gate.mark_secondary_pending();
        assert!(gate.should_drain_secondary_pre_run());
        assert!(!gate.should_drain_secondary_pre_run());
    }

    #[test]
    fn disabled_gate_preserves_always_drain_behavior() {
        let gate = PreRunDrainGate::new(false);

        assert!(gate.should_drain_secondary_pre_run());
        assert!(gate.should_drain_secondary_pre_run());
        assert!(gate.should_drain_secondary_pre_run());
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
        } = config;
        if cpu_count <= 1 {
            return Self {
                shutdown: Arc::new(AtomicBool::new(false)),
                terminal: Arc::new(SecondaryTerminalSignal::new()),
                controls: Vec::new(),
                handles: Vec::new(),
            };
        }

        let shutdown = Arc::new(AtomicBool::new(false));
        let terminal = Arc::new(SecondaryTerminalSignal::new());
        let controls: Vec<_> = (1..cpu_count)
            .map(|index| Arc::new(VcpuControl::new(index)))
            .collect();
        let handles = controls
            .iter()
            .map(|control| {
                let control = Arc::clone(control);
                let controls_for_thread = controls.clone();
                let shutdown = Arc::clone(&shutdown);
                let terminal = Arc::clone(&terminal);
                let platform = Arc::clone(&platform);
                let pre_run_drain_gate = Arc::clone(&pre_run_drain_gate);
                let smp_trace = smp_trace.clone();
                thread::Builder::new()
                    .name(format!("bridgevm-hvf-vcpu{}", control.index))
                    .spawn(move || {
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
                            },
                        )
                    })
                    .expect("spawn secondary vCPU thread")
            })
            .collect();

        Self {
            shutdown,
            terminal,
            controls,
            handles,
        }
    }
    pub(crate) fn terminal_action(&self) -> Option<PsciTerminalAction> {
        self.terminal.action()
    }
    pub(crate) fn shutdown_and_join(self) -> (Vec<(u64, u64)>, bool) {
        self.shutdown.store(true, Ordering::SeqCst);
        for control in &self.controls {
            control.notify_shutdown();
        }
        for control in &self.controls {
            control.request_exit_if_published();
        }
        for handle in self.handles {
            handle.join().expect("join secondary vCPU thread");
        }
        let run_error = self
            .controls
            .iter()
            .any(|control| control.run_error.load(Ordering::SeqCst));
        let exit_counts = self
            .controls
            .iter()
            .map(|control| (control.index, control.exits.load(Ordering::SeqCst)))
            .collect();
        (exit_counts, run_error)
    }
}
