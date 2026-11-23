//! Automation device work under short platform guards. The agent console's
//! host work runs on its own thread; vCPU0 only exchanges its bytes here
//! (agent_console/service_thread.rs).

use super::automation_stop::AutomationStop;
use super::input_prepare::prepare_live_input;
use super::{BootGeneration, Probe};
use crate::agent_console::ServiceThread;
use crate::*;

impl BootGeneration {
    /// One automation pass on vCPU0: host input, serial triggers and the agent
    /// console's bytes under one hold; then BOOT_TIMER, a requested checkpoint,
    /// the xHCI triggers, framebuffer checkpoints and the display export under
    /// another. Returns the stop the pass decided, if any.
    pub(super) unsafe fn automation_tick(
        &mut self,
        probe: &mut Probe<'_>,
        input_wake: bool,
    ) -> Option<String> {
        let (vcpu, exits, last_pc) = (probe.vcpu, self.exits, self.last_pc);
        let smp_trace = self.smp_trace.as_deref();
        let automation = &mut self.automation;
        let live_input = &mut probe.services.live_input;
        let input_prepared = prepare_live_input(
            probe.platform,
            smp_trace,
            &mut automation.gate,
            live_input,
            input_wake,
        );
        let panicked = {
            let mut platform_guard =
                lock_platform(probe.platform, smp_trace, 0, HoldSite::Automation);
            let locked_platform = &mut *platform_guard;
            if input_prepared {
                live_input.apply_pending(
                    locked_platform,
                    probe.guest_ram,
                    std::time::Instant::now(),
                );
            }
            if serial_reached_linux_panic(
                locked_platform.uart_output(),
                &mut automation.serial_stop_scans,
            ) {
                true
            } else {
                for trigger in &mut automation.triggers.uart {
                    trigger.maybe_fire(locked_platform);
                }
                for trigger in &mut automation.triggers.xhci_hid_boot_key {
                    trigger.maybe_fire(locked_platform);
                }
                if let Some(service) = automation.agent_console.as_mut() {
                    service.exchange(locked_platform, probe.guest_ram);
                }
                false
            }
        };
        if panicked {
            return Some("serial reached Linux kernel panic".into());
        }
        // Boot telemetry on host time (boot_clocks).
        let times = crate::boot_clocks::tick_times(&crate::boot_clocks::Live);
        let mut platform_guard = lock_platform(probe.platform, smp_trace, 0, HoldSite::Automation);
        let locked_platform = &mut *platform_guard;
        let mut checkpoint_committed = false;
        if automation
            .agent_console
            .as_ref()
            .is_some_and(ServiceThread::desktop_ready)
        {
            automation.boot_timer.observe_agent_ready(times.telemetry, exits);
            checkpoint_committed = checkpoint_glue::checkpoint_if_requested(
                &[vcpu],
                std::slice::from_raw_parts(probe.ram, probe.config.ram_size),
                locked_platform,
            )
            .unwrap_or_else(|error| panic!("capture VM checkpoint: {error}"));
        }
        for trigger in &mut automation.triggers.xhci_setup_input {
            let now = std::time::Instant::now();
            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                locked_platform,
                probe.guest_ram,
                now,
                |snapshot_platform, label, mem| {
                    ramfb_dump::print_checkpoint_for_platform(label, snapshot_platform, mem);
                },
            );
            if let Some(deadline) = trigger.pending_host_wake_deadline_at(locked_platform, now) {
                if automation
                    .pointer_deadline_wake
                    .arm_at(deadline, &mut automation.setup_input_host_wake)
                {
                    println!("xHCI setup-input host wake armed for delayed trigger");
                }
            }
        }
        for trigger in &mut automation.triggers.xhci_pointer_input {
            let now = std::time::Instant::now();
            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                locked_platform,
                probe.guest_ram,
                now,
                |snapshot_platform, label, mem| {
                    ramfb_dump::print_checkpoint_for_platform(label, snapshot_platform, mem);
                },
            );
            if let Some(deadline) = trigger.pending_host_wake_deadline_at(locked_platform, now) {
                if automation
                    .pointer_deadline_wake
                    .arm_at(deadline, &mut automation.setup_input_host_wake)
                {
                    println!("xHCI pointer-input host wake armed for delayed trigger");
                }
            }
        }
        automation
            .pointer_deadline_wake
            .arm(locked_platform, &mut automation.setup_input_host_wake);
        // Framebuffer checkpoints read and write the frame after the lock
        // is released (display_checkpoints.rs).
        let checkpoints = crate::display_checkpoints::DisplayCheckpoints::take(
            locked_platform,
            &mut automation.ramfb_sample_loop,
            &mut automation.boot_timer,
            vcpu,
            exits,
            last_pc,
        );
        automation
            .live_display_exporter
            .export_due(locked_platform, std::time::Instant::now());
        let stop = AutomationStop::check(
            checkpoint_committed,
            probe.config.stop_on_linux,
            locked_platform.uart_output(),
            &mut automation.serial_stop_scans,
        );
        checkpoints.emit_after(
            platform_guard,
            &mut automation.boot_timer,
            &*probe.guest_ram,
            ramfb_dump::print_checkpoint_with_virtio_gpu,
        );
        stop.reason(&mut automation.ramfb_sample_loop, vcpu)
    }
}
