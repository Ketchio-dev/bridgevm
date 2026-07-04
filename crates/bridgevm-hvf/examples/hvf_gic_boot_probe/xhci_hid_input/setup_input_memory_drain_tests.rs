use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::setup_input::XhciSetupInputTrigger;
use super::test_support::{
    acknowledge_event_ring_dequeue, assert_success_dci3_transfer_event_for_trb,
    configure_dci3_interrupt_in_over_bar0, emit_uart, new_platform_and_ram, program_xhci_bar0,
    read_bytes, reset_xhci_host_controller_over_bar0, ring_dci3_doorbell, write_dci3_normal_trb,
    DCI3_KEY_BUFFER, DCI3_RELEASE_BUFFER, DCI3_RING, EVENT_RING, TRB_SIZE,
};

#[test]
fn xhci_setup_input_trigger_memory_drain_delivers_pending_dci3_when_fired() {
    // Given: the live trigger sees its marker while DCI3 interrupt-IN TRBs are already pending.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "enter",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();

    // When: setup-input fires through the memory-aware live trigger path.
    assert!(trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        Instant::now(),
        |label: &str, _mem| {
            checkpoints.push(label.to_string());
        },
    ));

    // Then: the queued Enter press and release are drained into the pending DCI3 buffers.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        DCI3_RING + TRB_SIZE,
    );
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 1);
    assert_eq!(stats.queued_reports, 2);
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string()
        ]
    );
    println!(
        "trigger memory drain stats: queued_reports={} emitted_key_reports={} emitted_release_reports={} labels={}",
        stats.queued_reports,
        stats.emitted_key_reports,
        stats.emitted_release_reports,
        checkpoints.join(",")
    );
}

#[test]
fn xhci_setup_input_shared_marker_triggers_wait_for_prior_sequence_to_drain() {
    // Given: three setup-input triggers share the same serial marker and become
    // ready while the DCI3 keyboard endpoint is paced to one report per tick.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    let marker = b"BdsDxe: starting Boot0003";
    let mut triggers = [
        XhciSetupInputTrigger::from_env_value_with_custom_marker("setup-input", "text:cmd", marker)
            .unwrap(),
        XhciSetupInputTrigger::from_env_value_with_custom_marker("setup-input-2", "enter", marker)
            .unwrap(),
        XhciSetupInputTrigger::from_env_value_with_custom_marker(
            "setup-input-3",
            "text:ipconfig,enter",
            marker,
        )
        .unwrap(),
    ];
    triggers[1].set_fire_delay_for_test(Duration::from_millis(10));
    triggers[2].set_fire_delay_for_test(Duration::from_millis(20));
    emit_uart(&mut platform, marker);

    let expected_usages = [
        0x06, 0x10, 0x07, 0x28, 0x0c, 0x13, 0x06, 0x12, 0x11, 0x09, 0x0c, 0x0a, 0x28,
    ];
    let report_count = expected_usages.len() * 2;
    for report_index in 0..report_count {
        write_dci3_normal_trb(
            &mut mem,
            DCI3_RING + (TRB_SIZE * report_index as u64),
            DCI3_KEY_BUFFER + (0x20 * report_index as u64),
        );
        assert!(mem.write_bytes(DCI3_KEY_BUFFER + (0x20 * report_index as u64), &[0xaa; 8]));
    }

    let base = Instant::now();
    platform.set_xhci_report_interval(Duration::from_millis(30));
    for tick in 0..80 {
        platform.set_host_now(base + Duration::from_millis(10 * tick));
        for trigger in &mut triggers {
            trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
                &mut platform,
                &mut mem,
                base + Duration::from_millis(10 * tick),
                |_label: &str, _mem| {},
            );
        }
        platform.drain_xhci_setup_input_reports(&mut mem);
    }

    // Then: the emitted DCI3 key reports are exactly the three setup-input
    // sequences in trigger order, with releases between keys and no stray usages.
    let mut observed_usages = Vec::new();
    for report_index in 0..report_count {
        let report = read_bytes(&mem, DCI3_KEY_BUFFER + (0x20 * report_index as u64), 8);
        if report_index % 2 == 0 {
            observed_usages.push(report[2]);
        } else {
            assert_eq!(report, [0; 8], "release report {report_index}");
        }
    }
    assert_eq!(observed_usages, expected_usages);

    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, expected_usages.len() as u64);
    assert_eq!(stats.queued_reports, report_count as u64);
    assert_eq!(stats.emitted_key_reports, expected_usages.len() as u64);
    assert_eq!(stats.emitted_release_reports, expected_usages.len() as u64);
    assert_eq!(stats.busy_rejections, 0);
}

#[test]
fn xhci_setup_input_trigger_records_fire_after_partial_memory_drain() {
    // Given: the trigger sees its marker with exactly one DCI3 TRB available.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "enter",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();

    // When: memory-aware firing can emit only the Enter press report.
    let first_fire = trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        Instant::now(),
        |label: &str, _mem| {
            checkpoints.push(label.to_string());
        },
    );

    // Then: a positive emission records the trigger as fired and leaves release
    // delivery to the normal host-tick drain path.
    assert!(
        first_fire,
        "partial setup-input emission must make the trigger one-shot"
    );
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    let stats_after_first_fire = platform.xhci_setup_input_report_stats();
    assert_eq!(stats_after_first_fire.queued_actions, 1);
    assert_eq!(stats_after_first_fire.queued_reports, 2);
    assert_eq!(stats_after_first_fire.emitted_key_reports, 1);
    assert_eq!(stats_after_first_fire.emitted_release_reports, 0);

    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    assert!(
        !trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
            &mut platform,
            &mut mem,
            Instant::now(),
            |label: &str, _mem| {
                checkpoints.push(label.to_string());
            },
        ),
        "already-fired trigger must not queue Enter again"
    );
    let stats_after_second_check = platform.xhci_setup_input_report_stats();
    assert_eq!(stats_after_second_check.queued_actions, 1);
    assert_eq!(stats_after_second_check.queued_reports, 2);

    assert!(platform.drain_xhci_setup_input_reports(&mut mem));
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        DCI3_RING + TRB_SIZE,
    );
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 1);
    assert_eq!(stats.queued_reports, 2);
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
    assert_eq!(
        checkpoints,
        [
            "setup-input-before".to_string(),
            "setup-input-after".to_string()
        ]
    );
    println!(
        "trigger partial memory drain stats: queued_reports={} emitted_key_reports={} emitted_release_reports={} labels={}",
        stats.queued_reports,
        stats.emitted_key_reports,
        stats.emitted_release_reports,
        checkpoints.join(",")
    );
}

#[test]
fn xhci_setup_input_trigger_retries_after_hcrst_clears_queued_but_unemitted_reports() {
    // Given: boot-key delivery already emitted one key/release pair.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    assert!(platform.queue_xhci_hid_boot_key_usage(0x2c).is_ok());
    ring_dci3_doorbell(&mut platform, &mut mem);
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 2);
    ring_dci3_doorbell(&mut platform, &mut mem);
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 3);
    let boot_stats = platform.xhci_setup_input_report_stats();
    assert_eq!(boot_stats.emitted_key_reports, 1);
    assert_eq!(boot_stats.emitted_release_reports, 1);
    assert!(mem.write_bytes(DCI3_RING, &[0; 16]));
    assert!(mem.write_bytes(DCI3_RING + TRB_SIZE, &[0; 16]));

    let mut trigger = XhciSetupInputTrigger::from_env_value_with_custom_marker(
        "setup-input",
        "enter",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");
    let mut checkpoints = Vec::new();

    // When: setup-input queues before delivery completes, then HCRST clears pending reports.
    let first_fire = trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
        &mut platform,
        &mut mem,
        Instant::now(),
        |label: &str, _mem| {
            checkpoints.push(label.to_string());
        },
    );
    assert!(
        !first_fire,
        "trigger must remain retryable until setup-input reports emit"
    );
    let stats_before_reset = platform.xhci_setup_input_report_stats();
    assert_eq!(stats_before_reset.queued_actions, 2);
    assert_eq!(stats_before_reset.queued_reports, 4);
    assert_eq!(stats_before_reset.emitted_key_reports, 1);
    assert_eq!(stats_before_reset.emitted_release_reports, 1);
    reset_xhci_host_controller_over_bar0(&mut platform, &mut mem);
    let next_event_index = configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    assert_eq!(next_event_index, 2);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER + 0x40);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER + 0x40);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0x40, &[0xcc; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER + 0x40, &[0xdd; 8]));

    // Then: evaluating the same trigger retries after reset and emits Enter press+release.
    assert!(
        trigger.maybe_fire_with_mem_and_ramfb_checkpoints_at(
            &mut platform,
            &mut mem,
            Instant::now(),
            |label: &str, _mem| {
                checkpoints.push(label.to_string());
            },
        ),
        "trigger should retry after HCRST clears queued-but-unemitted setup input"
    );
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x40, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER + 0x40, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * next_event_index),
        DCI3_RING,
    );
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * (next_event_index + 1)),
        DCI3_RING + TRB_SIZE,
    );
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 3);
    assert_eq!(stats.queued_reports, 6);
    assert_eq!(stats.emitted_key_reports, 2);
    assert_eq!(stats.emitted_release_reports, 2);
    assert_eq!(checkpoints, ["setup-input-before".to_string()]);
    println!(
        "trigger retry after HCRST stats: queued_reports={} emitted_key_reports={} emitted_release_reports={} labels={}",
        stats.queued_reports,
        stats.emitted_key_reports,
        stats.emitted_release_reports,
        checkpoints.join(",")
    );
}
