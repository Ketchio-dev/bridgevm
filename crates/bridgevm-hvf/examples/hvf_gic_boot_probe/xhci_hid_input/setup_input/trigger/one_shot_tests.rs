use std::time::Instant;

use bridgevm_hvf::xhci::XhciSetupInputQueueError;

use super::*;
use crate::xhci_hid_input::test_support::{
    acknowledge_event_ring_dequeue, configure_dci3_interrupt_in_over_bar0, emit_uart, new_platform,
    new_platform_and_ram, program_xhci_bar0, reset_xhci_host_controller_over_bar0,
    ring_dci3_doorbell, write_dci3_normal_trb, DCI3_KEY_BUFFER, DCI3_RING,
};

#[test]
fn xhci_setup_input_busy_rejection_does_not_retry_after_queue_drains() {
    // Given: the setup-input marker is visible with zero fire delay.
    let mut trigger = XhciSetupInputTrigger::from_env_value("setup-input", "space").unwrap();
    let mut platform = new_platform();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let now = Instant::now();
    let mut queue_attempts = 0;

    // When: the first queue attempt is rejected as Busy.
    let first_poll_fired = trigger.maybe_fire_by_at(&mut platform, now, |_platform, _actions| {
        queue_attempts += 1;
        Err(XhciSetupInputQueueError::Busy)
    });

    // Then: the rejection is terminal for retry purposes.
    assert!(!first_poll_fired);
    assert!(!trigger.fired);
    assert!(trigger.attempted);
    assert!(
        !trigger.maybe_fire_by_at(&mut platform, now, |_platform, _actions| {
            queue_attempts += 1;
            Ok(())
        })
    );
    assert!(!trigger.fired);
    assert!(trigger.attempted);
    assert_eq!(queue_attempts, 1);
}

#[test]
fn xhci_setup_input_delayed_report_emit_marks_fired_and_blocks_hcrst_retry() {
    // Given: the setup-input marker is visible before Windows has armed DCI3 reports.
    let mut trigger = XhciSetupInputTrigger::from_env_value("setup-input", "space").unwrap();
    let (mut platform, mut ram) = new_platform_and_ram();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let now = Instant::now();

    // When: the queue accepts actions, but no report is emitted immediately.
    let first_poll_fired = trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut ram,
        now,
        |_platform, _label, _mem| {},
    );

    // Then: that attempt is terminal and later polls do not turn it into Busy retry noise.
    assert!(!first_poll_fired);
    assert!(!trigger.fired);
    assert!(trigger.attempted);
    let after_first = platform.xhci_setup_input_report_stats();
    assert_eq!(after_first.queued_actions, 1);
    assert_eq!(after_first.busy_rejections, 0);

    // When: the guest posts a DCI3 TRB later and the queued report emits.
    program_xhci_bar0(&mut platform, &mut ram);
    let event_index = configure_dci3_interrupt_in_over_bar0(&mut platform, &mut ram);
    write_dci3_normal_trb(&mut ram, DCI3_RING, DCI3_KEY_BUFFER);
    ring_dci3_doorbell(&mut platform, &mut ram);
    acknowledge_event_ring_dequeue(&mut platform, &mut ram, event_index);

    // Then: a later trigger poll observes the delayed emission and closes the trigger.
    assert!(!trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut ram,
        now,
        |_platform, _label, _mem| {}
    ));
    let after_second = platform.xhci_setup_input_report_stats();
    assert_eq!(after_second.queued_actions, 1);
    assert_eq!(after_second.busy_rejections, 0);
    assert_eq!(after_second.emitted_key_reports, 1);
    assert!(trigger.fired);

    // Then: HCRST after delayed emission does not reopen the already-fired trigger.
    reset_xhci_host_controller_over_bar0(&mut platform, &mut ram);
    assert!(!trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut ram,
        now,
        |_platform, _label, _mem| {}
    ));
    let after_reset = platform.xhci_setup_input_report_stats();
    assert_eq!(after_reset.queued_actions, 1);
    assert_eq!(after_reset.busy_rejections, 0);
}

#[test]
fn xhci_setup_input_fired_trigger_does_not_retry_after_hcrst() {
    // Given: the trigger has already fired successfully in one controller generation.
    let mut trigger = XhciSetupInputTrigger::from_env_value("setup-input", "space").unwrap();
    let (mut platform, mut ram) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut ram);
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let now = Instant::now();
    let mut queue_attempts = 0;
    assert!(
        trigger.maybe_fire_by_at(&mut platform, now, |_platform, _actions| {
            queue_attempts += 1;
            Ok(())
        })
    );
    assert!(trigger.fired);
    reset_xhci_host_controller_over_bar0(&mut platform, &mut ram);

    // When: the same trigger is evaluated after HCRST advances the controller generation.
    let fired_after_hcrst = trigger.maybe_fire_by_at(&mut platform, now, |_platform, _actions| {
        queue_attempts += 1;
        Ok(())
    });

    // Then: fired remains terminal and no second queue attempt is made.
    assert!(!fired_after_hcrst);
    assert!(trigger.fired);
    assert_eq!(queue_attempts, 1);
}
