use super::platform_setup_input_support::*;
use super::platform_test_support::*;
use super::XhciController;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::SetupInputAction;

const TRB_TYPE_LINK: u32 = 6;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;

#[test]
fn platform_delayed_setup_input_resyncs_reusable_dci3_ring_after_cycle_toggle() {
    // Given: Windows has wrapped a reusable DCI3 ring through a link TRB, leaving the
    // controller parked at ring base with the consumer cycle toggled away from base.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    write_dci3_link_trb(&mut mem, DCI3_RING + (TRB_SIZE * 2), DCI3_RING);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    ring_dci3_doorbell(&mut platform, &mut mem);
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 2);
    ring_dci3_doorbell(&mut platform, &mut mem);
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 3);
    ring_dci3_doorbell(&mut platform, &mut mem);

    // When: delayed setup input fires after that idle cycle-toggled state.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(&[SetupInputAction::Enter], &mut mem),
        Ok(())
    );

    // Then: queue-time drain resynchronizes to the reusable base TRB and emits both reports.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 3), DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 4),
        DCI3_RING + TRB_SIZE,
    );
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
}

#[test]
fn controller_post_fire_setup_input_resyncs_stale_dci3_base_cycle() {
    // Given: the live probe has a reusable DCI3 base TRB available, but the
    // controller-side consumer cycle is stale after prior DCI3/event-ring churn.
    let (_platform, mut mem) = new_platform_and_ram();
    write_event_ring_table(&mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    let mut controller = XhciController::new();
    controller.erstsz0 = 1;
    controller.erstba0 = ERST;
    controller.slot1_dci3_ring_base = DCI3_RING;
    controller.slot1_dci3_dequeue = DCI3_RING;
    controller.slot1_dci3_dcs = false;

    // When: setup-input fires after that idle state and tries to drain immediately.
    assert_eq!(
        controller.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // Then: queue-time drain resynchronizes to the available base TRBs instead of
    // leaving the just-queued setup-input reports stuck.
    assert!(controller.process_queued_dci3_input(&mut mem));
    assert!(controller.process_queued_dci3_input(&mut mem));
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    let stats = controller.setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
}

fn write_dci3_link_trb(mem: &mut impl GuestMemoryMut, trb_gpa: u64, target_gpa: u64) {
    assert!(mem.write_bytes(trb_gpa, &target_gpa.to_le_bytes()));
    assert!(mem.write_bytes(trb_gpa + 8, &0u32.to_le_bytes()));
    assert!(mem.write_bytes(
        trb_gpa + 12,
        &((TRB_TYPE_LINK << 10) | TRB_LINK_TOGGLE_CYCLE | 1).to_le_bytes(),
    ));
}
