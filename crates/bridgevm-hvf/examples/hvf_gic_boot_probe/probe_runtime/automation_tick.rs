//! Automation device work brackets host-only agent progression with short platform guards.

macro_rules! automation_tick {
    ($platform:ident, $smp_trace:ident, $automation_gate:ident, $live_input:ident, $guest_ram:ident, $input_wake:ident, $serial_stop_scans:ident, $uart_triggers:ident, $xhci_hid_boot_key_triggers:ident, $agent_console:ident, $boot_timer:ident, $exits:ident, $vcpu:ident, $ram:ident, $ram_size:ident, $xhci_setup_input_triggers:ident, $pointer_deadline_wake:ident, $setup_input_host_wake:ident, $xhci_pointer_input_triggers:ident, $ramfb_sample_loop:ident, $live_display_exporter:ident, $last_pc:ident, $stop_on_linux:ident $(,)?) => {{
        let panicked = {
            let mut platform_guard =
                lock_platform(&$platform, $smp_trace.as_deref(), 0, HoldSite::Automation);
            let locked_platform = &mut *platform_guard;
            $automation_gate.note_checked();
            if let Some(d) = locked_platform.xhci_pointer_report_deadline() {
                pointer_deadline_trace::report_overdue(d);
            }
            let now = std::time::Instant::now();
            $live_input.tick(locked_platform, &mut $guest_ram, now, $input_wake);
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
            let now = std::time::Instant::now();
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
                $boot_timer.observe_agent_ready(now, $exits);
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
            $ramfb_sample_loop.emit_due($vcpu, |label| {
                ramfb_dump::print_checkpoint_for_platform(label, locked_platform, &$guest_ram);
            });
            $live_display_exporter.export_due(locked_platform, std::time::Instant::now());
            $boot_timer.tick(locked_platform, &$guest_ram, $exits, $last_pc);
            if checkpoint_committed {
                Some("VM checkpoint committed; suspended process exiting".into())
            } else if $stop_on_linux
                && serial_reached_linux_early_boot(
                    locked_platform.uart_output(),
                    &mut $serial_stop_scans,
                )
            {
                Some("serial reached Linux early boot".into())
            } else if serial_reached_shell(locked_platform.uart_output(), &mut $serial_stop_scans) {
                match $ramfb_sample_loop.observe_shell($vcpu) {
                    RamfbSampleShellAction::Continue => None,
                    RamfbSampleShellAction::StopNow { reason } => Some(reason.into()),
                }
            } else {
                None
            }
        }
    }};
}
pub(crate) use automation_tick;
