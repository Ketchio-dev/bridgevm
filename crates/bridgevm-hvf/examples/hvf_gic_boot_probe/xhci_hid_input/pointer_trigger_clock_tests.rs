use super::pointer_input::XhciPointerInputTrigger;
use super::test_support::{
    acknowledge_event_ring_dequeue, configure_dci3_and_dci5_interrupt_in_over_bar0, emit_uart,
    new_platform_and_ram, program_xhci_bar0, write_dci5_normal_trb, DCI5_POINTER_BUFFER,
    DCI5_RING, TRB_SIZE,
};
use std::time::{Duration, Instant};
#[test]
fn pointer_trigger_uses_its_actual_now_for_first_emission_pacing() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    let event_index = configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..2 {
        write_dci5_normal_trb(
            &mut mem,
            DCI5_RING + TRB_SIZE * index,
            DCI5_POINTER_BUFFER + 0x20 * index,
        );
    }
    platform.set_xhci_report_interval(Duration::from_millis(1000));
    let base = Instant::now();
    platform.set_host_now(base); // deliberately stale relative to trigger
    let mut trigger = XhciPointerInputTrigger::from_env_value_with_custom_marker(
        "pointer-input", "click:center", b"READY",
    ).unwrap();
    emit_uart(&mut platform, b"READY");
    let fired_at = base + Duration::from_millis(900);
    assert!(trigger.maybe_fire_with_mem_at(&mut platform, &mut mem, fired_at));
    assert_eq!(trigger.pending_host_wake_deadline_at(&platform, fired_at), Some(fired_at + Duration::from_millis(1000)));
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, event_index + 1);
    platform.set_host_now(base + Duration::from_millis(1000)); // stale-base boundary
    platform.drain_xhci_pointer_input_reports(&mut mem);
    assert_eq!(platform.xhci_pointer_input_report_stats().emitted_release_reports, 0);
    platform.set_host_now(fired_at + Duration::from_millis(1000));
    assert!(platform.drain_xhci_pointer_input_reports(&mut mem));
    assert_eq!(platform.xhci_pointer_input_report_stats().emitted_release_reports, 1);
}
#[path = "pointer_deadline_tests.rs"]
mod pointer_deadline_tests;
