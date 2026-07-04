use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::pointer_input::{XhciPointerInputEnvError, XhciPointerInputTrigger};
use super::test_support::{
    assert_success_dci5_transfer_event_for_trb, configure_dci3_and_dci5_interrupt_in_over_bar0,
    emit_uart, new_platform_and_ram, program_xhci_bar0, read_bytes, write_dci5_normal_trb,
    DCI5_POINTER_BUFFER, DCI5_RING, EVENT_RING, TRB_SIZE,
};

#[test]
fn xhci_pointer_input_parser_accepts_move_and_click_coordinates() {
    let trigger =
        XhciPointerInputTrigger::from_env_value("pointer-input", "move:1x2 click:center").unwrap();

    assert_eq!(trigger.action_names(), "move:1x2,click:16383x16383");
}

#[test]
fn xhci_pointer_input_parser_rejects_invalid_coordinates() {
    let error = XhciPointerInputTrigger::from_env_value("pointer-input", "move:left").unwrap_err();

    assert_eq!(error.name(), "invalid_coordinate");
}

#[test]
fn xhci_pointer_input_parser_rejects_out_of_range_coordinates() {
    let error =
        XhciPointerInputTrigger::from_env_value("pointer-input", "click:32768x1").unwrap_err();

    assert_eq!(
        error,
        XhciPointerInputEnvError::CoordinateOutOfRange {
            token: "click:32768x1".to_string()
        }
    );
}

#[test]
fn xhci_pointer_input_trigger_memory_drain_delivers_pending_dci5_when_fired() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    let next_event_index = configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci5_normal_trb(&mut mem, DCI5_RING, DCI5_POINTER_BUFFER);
    assert!(mem.write_bytes(DCI5_POINTER_BUFFER, &[0xaa; 8]));
    let mut trigger = XhciPointerInputTrigger::from_env_value_with_custom_marker(
        "pointer-input",
        "move:16384x8192",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

    assert!(trigger.maybe_fire_with_mem_at(&mut platform, &mut mem, Instant::now()));

    assert_eq!(
        read_bytes(&mem, DCI5_POINTER_BUFFER, 5),
        [0, 0, 0x40, 0, 0x20]
    );
    assert_success_dci5_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * next_event_index),
        DCI5_RING,
    );
    let stats = platform.xhci_pointer_input_report_stats();
    assert_eq!(stats.queued_actions, 1);
    assert_eq!(stats.queued_reports, 1);
    assert_eq!(stats.emitted_move_reports, 1);
    assert!(trigger.fired());
}

#[test]
fn xhci_pointer_input_trigger_records_fire_after_delayed_dci5_emit() {
    let (mut platform, mut mem) = new_platform_and_ram();
    let mut trigger = XhciPointerInputTrigger::from_env_value_with_custom_marker(
        "pointer-input",
        "click:center",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

    assert!(!trigger.maybe_fire_with_mem_at(&mut platform, &mut mem, Instant::now()));
    assert!(!trigger.fired());
    let queued = platform.xhci_pointer_input_report_stats();
    assert_eq!(queued.queued_actions, 1);
    assert_eq!(queued.queued_reports, 2);
    assert_eq!(queued.emitted_button_reports, 0);

    program_xhci_bar0(&mut platform, &mut mem);
    let next_event_index = configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci5_normal_trb(&mut mem, DCI5_RING, DCI5_POINTER_BUFFER);
    assert!(mem.write_bytes(DCI5_POINTER_BUFFER, &[0xbb; 8]));
    assert!(platform.drain_xhci_pointer_input_reports(&mut mem));

    assert!(!trigger.maybe_fire_with_mem_at(&mut platform, &mut mem, Instant::now()));
    assert!(trigger.fired());
    assert_eq!(
        read_bytes(&mem, DCI5_POINTER_BUFFER, 5),
        [1, 0xff, 0x3f, 0xff, 0x3f]
    );
    let stats = platform.xhci_pointer_input_report_stats();
    assert_eq!(stats.emitted_button_reports, 1);
    assert_eq!(stats.emitted_release_reports, 0);
    assert_success_dci5_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * next_event_index),
        DCI5_RING,
    );
}

#[test]
fn xhci_pointer_input_ramfb_emits_before_after_and_delays_when_trigger_fires() {
    // Given: pointer-input has two configured delayed RAMFB checkpoints.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci5_normal_trb(&mut mem, DCI5_RING, DCI5_POINTER_BUFFER);
    let mut trigger = XhciPointerInputTrigger::from_env_value_with_ramfb_delay_ms(
        "pointer-input",
        "move:16384x8192",
        &[5, 15],
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let start = Instant::now();
    let mut checkpoints = Vec::new();

    // When: pointer-input fires and the live loop polls before, at, and after each delay.
    trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        start,
        |label: &str, _mem| {
            checkpoints.push(label.to_string());
        },
    );
    trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        start + Duration::from_millis(5),
        |label: &str, _mem| {
            checkpoints.push(label.to_string());
        },
    );
    trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        start + Duration::from_millis(20),
        |label: &str, _mem| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: immediate labels are unchanged and each delayed label appears exactly once.
    assert_eq!(
        checkpoints,
        [
            "pointer-input-before".to_string(),
            "pointer-input-after".to_string(),
            "pointer-input-delay-5ms".to_string(),
            "pointer-input-delay-15ms".to_string()
        ]
    );
    println!("pointer delay checkpoint labels: {}", checkpoints.join(","));
}
