//! Automation device work brackets host-only agent progression with short platform guards.

macro_rules! automation_tick {
    ($platform:ident, $smp_trace:ident, $automation_gate:ident, $live_input:ident, $guest_ram:ident, $input_wake:ident, $serial_stop_scans:ident, $uart_triggers:ident, $xhci_hid_boot_key_triggers:ident, $agent_console:ident, $boot_timer:ident, $exits:ident, $vcpu:ident, $ram:ident, $ram_size:ident, $xhci_setup_input_triggers:ident, $pointer_deadline_wake:ident, $setup_input_host_wake:ident, $xhci_pointer_input_triggers:ident, $ramfb_sample_loop:ident, $live_display_exporter:ident, $last_pc:ident, $stop_on_linux:ident $(,)?) => {{
        let input_prepared = $crate::probe_runtime::input_prepare::prepare_live_input!(
            $platform,
            $smp_trace,
            $automation_gate,
            $live_input,
            $input_wake
        );
        let panicked = {
            let mut platform_guard =
                lock_platform(&$platform, $smp_trace.as_deref(), 0, HoldSite::Automation);
            let locked_platform = &mut *platform_guard;
            if input_prepared {
                $live_input.apply_pending(
                    locked_platform,
                    &mut $guest_ram,
                    std::time::Instant::now(),
                );
            }
            if serial_reached_linux_panic(locked_platform.uart_output(), &mut $serial_stop_scans) {
                true
            } else {
                for trigger in &mut $uart_triggers {
                    trigger.maybe_fire(locked_platform);
                }
                for trigger in &mut $xhci_hid_boot_key_triggers {
                    trigger.maybe_fire(locked_platform);
                }
                if let Some(harness) = $agent_console.as_mut() {
                    harness.capture_inbound(locked_platform);
                }
                false
            }
        };
        if panicked {
            Some("serial reached Linux kernel panic".into())
        } else {
            // The agent console on VM time, boot telemetry on host time (boot_clocks).
            let times = crate::boot_clocks::tick_times(&crate::boot_clocks::Live);
            let now = times.agent;
            if let Some(harness) = $agent_console.as_mut() {
                harness.advance(now, &mut |bytes| {
                    let mut platform_guard =
                        lock_platform(&$platform, $smp_trace.as_deref(), 0, HoldSite::Automation);
                    platform_guard.virtio_console_agent_send(bytes, &mut $guest_ram);
                });
            }
            let mut platform_guard =
                lock_platform(&$platform, $smp_trace.as_deref(), 0, HoldSite::Automation);
            let locked_platform = &mut *platform_guard;
            let mut checkpoint_committed = false;
            if $agent_console
                .as_ref()
                .is_some_and(AgentConsoleHarness::desktop_ready)
            {
                $boot_timer.observe_agent_ready(times.telemetry, $exits);
                checkpoint_committed = checkpoint_glue::checkpoint_if_requested(
                    &[$vcpu],
                    std::slice::from_raw_parts($ram, $ram_size),
                    locked_platform,
                )
                .unwrap_or_else(|error| panic!("capture VM checkpoint: {error}"));
            }
            for trigger in &mut $xhci_setup_input_triggers {
                let now = std::time::Instant::now();
                trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                    locked_platform,
                    &mut $guest_ram,
                    now,
                    |snapshot_platform, label, mem| {
                        ramfb_dump::print_checkpoint_for_platform(label, snapshot_platform, mem);
                    },
                );
                if let Some(deadline) = trigger.pending_host_wake_deadline_at(locked_platform, now)
                {
                    if $pointer_deadline_wake.arm_at(deadline, &mut $setup_input_host_wake) {
                        println!("xHCI setup-input host wake armed for delayed trigger");
                    }
                }
            }
            for trigger in &mut $xhci_pointer_input_triggers {
                let now = std::time::Instant::now();
                trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                    locked_platform,
                    &mut $guest_ram,
                    now,
                    |snapshot_platform, label, mem| {
                        ramfb_dump::print_checkpoint_for_platform(label, snapshot_platform, mem);
                    },
                );
                if let Some(deadline) = trigger.pending_host_wake_deadline_at(locked_platform, now)
                {
                    if $pointer_deadline_wake.arm_at(deadline, &mut $setup_input_host_wake) {
                        println!("xHCI pointer-input host wake armed for delayed trigger");
                    }
                }
            }
            $pointer_deadline_wake.arm(locked_platform, &mut $setup_input_host_wake);
            // Framebuffer checkpoints read and write the frame after the lock
            // is released (display_checkpoints.rs).
            let checkpoints = $crate::display_checkpoints::DisplayCheckpoints::take(
                locked_platform,
                &mut $ramfb_sample_loop,
                &mut $boot_timer,
                $vcpu,
                $exits,
                $last_pc,
            );
            $live_display_exporter.export_due(locked_platform, std::time::Instant::now());
            let stop = $crate::probe_runtime::automation_stop::AutomationStop::check(
                checkpoint_committed,
                $stop_on_linux,
                locked_platform.uart_output(),
                &mut $serial_stop_scans,
            );
            checkpoints.emit_after(
                platform_guard,
                &mut $boot_timer,
                &$guest_ram,
                ramfb_dump::print_checkpoint_with_virtio_gpu,
            );
            stop.reason(&mut $ramfb_sample_loop, $vcpu)
        }
    }};
}
pub(crate) use automation_tick;
