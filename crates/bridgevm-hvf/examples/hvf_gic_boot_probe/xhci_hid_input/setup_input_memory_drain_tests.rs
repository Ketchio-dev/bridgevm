use std::time::Instant;

use super::setup_input::XhciSetupInputTrigger;
use super::test_support::{
    assert_success_dci3_transfer_event_for_trb, configure_dci3_interrupt_in_over_bar0, emit_uart,
    new_platform_and_ram, program_xhci_bar0, read_bytes, write_dci3_normal_trb, DCI3_KEY_BUFFER,
    DCI3_RELEASE_BUFFER, DCI3_RING, EVENT_RING, TRB_SIZE,
};

#[test]
fn xhci_setup_input_trigger_memory_drain_delivers_pending_dci3_when_fired() {
    // Given: the live trigger sees its marker while DCI3 interrupt-IN TRBs are already pending.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(bridgevm_hvf::fwcfg::GuestMemoryMut::write_bytes(
        &mut mem,
        DCI3_KEY_BUFFER,
        &[0xaa; 8]
    ));
    assert!(bridgevm_hvf::fwcfg::GuestMemoryMut::write_bytes(
        &mut mem,
        DCI3_RELEASE_BUFFER,
        &[0xbb; 8]
    ));
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
