use super::configure_endpoint_tests::*;
use super::test_support::{
    command_control, setup_command_rings_with_parameter, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID,
    TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const READDRESS_INPUT_CONTEXT: u64 = 0x7200;
const READDRESS_EP0_RING: u64 = 0x7300;
const READDRESS_OUTPUT_CONTEXT: u64 = 0x7600;
const SECOND_READDRESS_OUTPUT_CONTEXT: u64 = 0x7800;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;

#[test]
fn delayed_setup_input_after_readdress_drains_from_current_dci3_output_dequeue() {
    // Given: DCI3 emitted the boot-key pair, leaving the output context at the next live TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    for report_index in 0..16 {
        let trb = DCI3_RING + (TRB_SIZE * (report_index + 2));
        let buffer = delayed_setup_input_buffer(report_index);
        write_dci3_normal_trb(&mut mem, trb, buffer, true);
        assert!(mem.write_bytes(buffer, &[0xcc; 8]));
    }
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 2));

    // When: Windows re-addresses slot 1 via EP0, then delayed setup-input queues eight actions.
    setup_address_device_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        mem.read_u64(OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_RING + (TRB_SIZE * 2) + TRB_CYCLE
    );
    xhci.slot1_dci3_dequeue = 0;
    xhci.slot1_dci3_ring_base = 0;
    xhci.slot1_dci3_dcs = false;
    xhci.slot1_dci3_two_entry_queue_rearm = false;
    let delayed_actions = [
        SetupInputAction::Enter,
        SetupInputAction::Tab,
        SetupInputAction::Enter,
        SetupInputAction::Space,
        SetupInputAction::Tab,
        SetupInputAction::Enter,
        SetupInputAction::Space,
        SetupInputAction::Enter,
    ];
    assert_eq!(xhci.queue_setup_input_actions(&delayed_actions), Ok(()));
    for _ in 0..16 {
        let drained = xhci.process_queued_dci3_input(&mut mem);
        let stats = xhci.setup_input_report_stats();
        assert!(
            drained,
            "process_queued_dci3_input returned false after readdress: dequeue={:#x} ring_base={:#x} dcs={} stats={stats:?}",
            xhci.slot1_dci3_dequeue,
            xhci.slot1_dci3_ring_base,
            xhci.slot1_dci3_dcs
        );
    }

    // Then: delayed setup input drains from the current DCI3 output dequeue, not the stale start.
    for (action_index, action) in delayed_actions.iter().enumerate() {
        let key_report_index = (action_index as u64) * 2;
        assert_eq!(
            mem.read_bytes(delayed_setup_input_buffer(key_report_index), 8)
                .unwrap(),
            [0, 0, action.usage(), 0, 0, 0, 0, 0]
        );
        assert_eq!(
            mem.read_bytes(delayed_setup_input_buffer(key_report_index + 1), 8)
                .unwrap(),
            [0; 8]
        );
    }
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.queued_reports, 18);
    assert_eq!(stats.emitted_key_reports, 9);
    assert_eq!(stats.emitted_release_reports, 9);
}

#[test]
fn delayed_setup_input_after_new_output_context_readdress_keeps_dci3_endpoint() {
    // Given: DCI3 emitted the boot-key pair, leaving valid internal endpoint state.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    write_dci3_normal_trb(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 2),
        delayed_setup_input_buffer(0),
        true,
    );
    write_dci3_normal_trb(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 3),
        delayed_setup_input_buffer(1),
        true,
    );
    assert!(mem.write_bytes(delayed_setup_input_buffer(0), &[0xcc; 8]));
    assert!(mem.write_bytes(delayed_setup_input_buffer(1), &[0xdd; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 2));

    // When: the guest switches slot 1 to a fresh output context and re-addresses via EP0.
    mem.write_u64(
        DCBAA + (u64::from(ENABLE_SLOT_ID) * 8),
        READDRESS_OUTPUT_CONTEXT,
    );
    setup_address_device_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // Then: queued setup-input can still drain through DCI3 in the current output context.
    let drained = xhci.process_queued_dci3_input(&mut mem);
    assert!(
        drained,
        "reason=no_dci3_endpoint dequeue={:#x} ring_base={:#x} dcs={} stats={:?}",
        xhci.slot1_dci3_dequeue,
        xhci.slot1_dci3_ring_base,
        xhci.slot1_dci3_dcs,
        xhci.setup_input_report_stats()
    );
    assert_eq!(
        mem.read_bytes(delayed_setup_input_buffer(0), 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
}

#[test]
fn delayed_setup_input_after_repeated_fresh_output_readdress_keeps_live_dci3_endpoint() {
    // Given: DCI3 emitted the boot-key pair, leaving the live ring at the next TRB.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    write_dci3_normal_trb(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 2),
        delayed_setup_input_buffer(0),
        true,
    );
    write_dci3_normal_trb(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 3),
        delayed_setup_input_buffer(1),
        true,
    );
    assert!(mem.write_bytes(delayed_setup_input_buffer(0), &[0xcc; 8]));
    assert!(mem.write_bytes(delayed_setup_input_buffer(1), &[0xdd; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 2));

    // When: Windows-style readdress loops move slot 1 across two fresh output contexts.
    mem.write_u64(
        DCBAA + (u64::from(ENABLE_SLOT_ID) * 8),
        READDRESS_OUTPUT_CONTEXT,
    );
    setup_address_device_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        mem.read_u64(READDRESS_OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET),
        DCI3_RING + (TRB_SIZE * 2) + TRB_CYCLE
    );

    mem.write_u64(
        DCBAA + (u64::from(ENABLE_SLOT_ID) * 8),
        SECOND_READDRESS_OUTPUT_CONTEXT,
    );
    setup_address_device_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // Then: delayed setup-input drains from the live DCI3 ring, not a zeroed endpoint snapshot.
    let drained = xhci.process_queued_dci3_input(&mut mem);
    assert!(
        drained,
        "reason=no_dci3_endpoint dequeue={:#x} ring_base={:#x} current_output_dequeue={:#x} stats={:?}",
        xhci.slot1_dci3_dequeue,
        xhci.slot1_dci3_ring_base,
        mem.read_u64(
            SECOND_READDRESS_OUTPUT_CONTEXT + DCI3_OUTPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET
        ),
        xhci.setup_input_report_stats()
    );
    assert_eq!(
        mem.read_bytes(delayed_setup_input_buffer(0), 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
}

fn setup_address_device_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        READDRESS_INPUT_CONTEXT,
        command_control(TRB_TYPE_ADDRESS_DEVICE, ENABLE_SLOT_ID),
    );
    mem.write_u64(
        READDRESS_INPUT_CONTEXT + EP0_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        READDRESS_EP0_RING | TRB_CYCLE,
    );
}

fn delayed_setup_input_buffer(report_index: u64) -> u64 {
    DCI3_BUFFER + 0x40 + (0x20 * report_index)
}
