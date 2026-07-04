use super::platform_setup_input_support::*;
use super::platform_test_support::*;
use super::XhciController;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::SetupInputAction;

const TRB_TYPE_LINK: u32 = 6;
const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;

#[test]
fn platform_delayed_setup_input_resyncs_reusable_dci3_ring_after_cycle_toggle() {
    // Given: a reusable two-entry DCI3 ring whose link TRB wraps to ring base
    // and toggles the consumer cycle. With no report queued the endpoint NAKs
    // idle polls, so the ring stays armed until real input arrives.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    write_dci3_link_trb(&mut mem, DCI3_RING + (TRB_SIZE * 2), DCI3_RING);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));

    // An idle poll before any input pends: no transfer event and no report.
    ring_dci3_doorbell(&mut platform, &mut mem);
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER, 8), [0xaa; 8]);
    assert_eq!(read_bytes(&mem, EVENT_RING + TRB_SIZE, 8), [0; 8]);

    // When: a delayed setup-input sequence of two actions drains at queue time.
    // Delivering four reports across the two-entry ring forces the drain to
    // follow the link, wrap to base, and resynchronize the toggled cycle.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(
            &[SetupInputAction::Enter, SetupInputAction::Tab],
            &mut mem
        ),
        Ok(())
    );

    // Then: every queued report is emitted; the last pass leaves Tab in the key
    // buffer and the release in the release buffer, and four transfer events
    // were posted onto the reused ring TRBs.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x2b, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        DCI3_RING + TRB_SIZE,
    );
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 3), DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 4),
        DCI3_RING + TRB_SIZE,
    );
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 2);
    assert_eq!(stats.emitted_release_reports, 2);
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
    controller.mmio_write(0x1028, 4, 1);
    controller.mmio_write(0x1030, 8, ERST);
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

#[test]
fn controller_second_setup_input_rearms_two_entry_reusable_ring_after_first_sequence() {
    // Given: the first setup-input sequence consumed a two-entry DCI3 ring.
    let (_platform, mut mem) = new_platform_and_ram();
    write_event_ring_table(&mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    let mut controller = XhciController::new();
    controller.mmio_write(0x1028, 4, 1);
    controller.mmio_write(0x1030, 8, ERST);
    controller.slot1_dci3_ring_base = DCI3_RING;
    controller.slot1_dci3_dequeue = DCI3_RING;
    controller.slot1_dci3_dcs = true;
    assert_eq!(
        controller.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );
    assert!(controller.process_queued_dci3_input(&mut mem));
    assert!(controller.process_queued_dci3_input(&mut mem));

    // When: Windows reuses the same two-entry ring with the opposite cycle state
    // for a second setup-input sequence.
    write_dci3_normal_trb_with_cycle(&mut mem, DCI3_RING, DCI3_KEY_BUFFER + 0x40, false);
    write_dci3_normal_trb_with_cycle(
        &mut mem,
        DCI3_RING + TRB_SIZE,
        DCI3_RELEASE_BUFFER + 0x40,
        false,
    );
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0x40, &[0xcc; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER + 0x40, &[0xdd; 8]));
    assert_eq!(
        controller.queue_setup_input_actions(&[SetupInputAction::Space]),
        Ok(())
    );

    // Then: queued-drain re-arms to the reused ring base instead of leaving the
    // second sequence stuck behind a cycle mismatch.
    assert!(controller.process_queued_dci3_input(&mut mem));
    assert!(controller.process_queued_dci3_input(&mut mem));
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x40, 8),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER + 0x40, 8), [0; 8]);
    let stats = controller.setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 2);
    assert_eq!(stats.emitted_release_reports, 2);
}

#[test]
fn controller_second_setup_input_rearms_from_live_dequeue_when_ring_base_is_stale() {
    // Given: queued setup-input is armed for a two-entry reusable DCI3 ring, but
    // the original ring base has gone stale while the live dequeue still points
    // at valid TRBs for the pending key/release reports.
    let (_platform, mut mem) = new_platform_and_ram();
    write_event_ring_table(&mut mem);
    write_dci3_unsupported_trb(&mut mem, DCI3_RING);
    let live_dequeue = DCI3_RING + (TRB_SIZE * 2);
    write_dci3_normal_trb(&mut mem, live_dequeue, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, live_dequeue + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));
    let mut controller = XhciController::new();
    controller.mmio_write(0x1028, 4, 1);
    controller.mmio_write(0x1030, 8, ERST);
    controller.slot1_dci3_ring_base = DCI3_RING;
    controller.slot1_dci3_dequeue = live_dequeue;
    controller.slot1_dci3_dcs = false;
    controller.slot1_dci3_two_entry_queue_rearm = true;
    controller.slot1_dci3_last_dequeue = live_dequeue;
    controller.slot1_dci3_last_dcs = true;
    controller.slot1_dci3_last_ring_base = DCI3_RING;
    controller.slot1_dci3_last_ring_dcs = true;
    controller.slot1_dci3_last_reusable = true;

    // When: delayed setup-input queues a second sequence after the ring base is stale.
    assert_eq!(
        controller.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // Then: queued drain should recover from the valid live dequeue instead of
    // refusing the stale unsupported ring-base TRB.
    assert!(
        controller.process_queued_dci3_input(&mut mem),
        "expected queued DCI3 setup-input to rearm from live_dequeue={live_dequeue:#x} \
         with stale ring_base={DCI3_RING:#x}; dequeue={:#x} dcs={} stats={:?}",
        controller.slot1_dci3_dequeue,
        controller.slot1_dci3_dcs,
        controller.setup_input_report_stats()
    );
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

fn write_dci3_normal_trb_with_cycle(
    mem: &mut impl GuestMemoryMut,
    trb_gpa: u64,
    buffer_gpa: u64,
    cycle: bool,
) {
    let control = (TRB_TYPE_NORMAL << 10) | u32::from(cycle);
    assert!(mem.write_bytes(trb_gpa, &buffer_gpa.to_le_bytes()));
    assert!(mem.write_bytes(trb_gpa + 8, &8u32.to_le_bytes()));
    assert!(mem.write_bytes(trb_gpa + 12, &control.to_le_bytes()));
}

fn write_dci3_unsupported_trb(mem: &mut impl GuestMemoryMut, trb_gpa: u64) {
    assert!(mem.write_bytes(trb_gpa, &DCI3_KEY_BUFFER.to_le_bytes()));
    assert!(mem.write_bytes(trb_gpa + 8, &8u32.to_le_bytes()));
    assert!(mem.write_bytes(trb_gpa + 12, &0u32.to_le_bytes()));
}
