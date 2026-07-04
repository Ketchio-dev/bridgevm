use super::configure_endpoint_tests::*;
use super::test_support::{
    command_control, setup_command_rings_with_parameter, TestRam, DOORBELL_BASE, ENABLE_SLOT_ID,
    EVENT_RING, TRB_SIZE, TRB_TYPE_ADDRESS_DEVICE,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

const READDRESS_INPUT_CONTEXT: u64 = 0x7200;
const READDRESS_EP0_RING: u64 = 0x7300;
const RECONFIGURE_INPUT_CONTEXT: u64 = 0x7400;
const NEW_DCI3_RING: u64 = 0x7600;
const NEW_DCI3_BUFFER: u64 = 0x7800;
const EP0_INPUT_CONTEXT_OFFSET: u64 = 0x40;

#[test]
fn slot1_dci3_doorbell_emits_queued_boot_key_report_once_then_releases() {
    // Given: Configure Endpoint installed DCI3 with two active interrupt IN buffers.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    assert!(!xhci.queue_boot_keyboard_space());

    // When: the guest polls DCI3 twice after the key was queued.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(!xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: Space is emitted exactly once and the next report releases the key.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 2));
}

#[test]
fn slot1_dci3_short_transfer_preserves_queued_setup_input_report_until_full_buffer() {
    // Given: Configure Endpoint installed DCI3 with a short interrupt IN buffer before full ones.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    mem.write_u32(DCI3_RING + 8, 4);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    write_dci3_normal_trb(
        &mut mem,
        DCI3_RING + (TRB_SIZE * 2),
        DCI3_BUFFER + 0x40,
        true,
    );
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(mem.write_bytes(DCI3_BUFFER + 0x40, &[0xcc; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());

    // When: the guest first polls with a short buffer, then with two full buffers.
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(!xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the short transfer does not expose or consume Space; the next full buffer gets it.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0, 0, 0xaa, 0xaa, 0xaa, 0xaa]
    );
    assert_eq!(
        mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x40, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
    assert_success_dci3_transfer_event(
        &mem,
        EVENT_RING + (TRB_SIZE * 3),
        DCI3_RING + (TRB_SIZE * 2),
    );
}

#[test]
fn setup_input_action_queue_emits_minimal_navigation_sequence() {
    // Given: Configure Endpoint installed DCI3 with one interrupt IN buffer per setup action edge.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    for index in 0..6 {
        let trb = DCI3_RING + (TRB_SIZE * index);
        let buffer = DCI3_BUFFER + (0x20 * index);
        write_dci3_normal_trb(&mut mem, trb, buffer, true);
        assert!(mem.write_bytes(buffer, &[0xaa; 8]));
    }
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: the minimal Windows Setup navigation sequence is queued and DCI3 is polled.
    assert_eq!(
        xhci.queue_setup_input_actions(&[
            SetupInputAction::Tab,
            SetupInputAction::Enter,
            SetupInputAction::Space,
        ]),
        Ok(())
    );
    for _ in 0..6 {
        assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    }

    // Then: each typed action emits one boot-keyboard report followed by release.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2b, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x20, 8).unwrap(), [0; 8]);
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER + 0x40, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x60, 8).unwrap(), [0; 8]);
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER + 0x80, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0xa0, 8).unwrap(), [0; 8]);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING + (TRB_SIZE * 6));
}

#[test]
fn setup_input_action_queue_emits_modified_key_report() {
    // Given: Configure Endpoint installed DCI3 with one interrupt IN buffer per setup action edge.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0xa000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    for index in 0..4 {
        let trb = DCI3_RING + (TRB_SIZE * index);
        let buffer = DCI3_BUFFER + (0x20 * index);
        write_dci3_normal_trb(&mut mem, trb, buffer, true);
        assert!(mem.write_bytes(buffer, &[0xaa; 8]));
    }
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));

    // When: a Windows Run shortcut and Enter are queued and DCI3 is polled.
    assert_eq!(
        xhci.queue_setup_input_actions(&[
            SetupInputAction::Key {
                name: "win+r",
                modifier: 0x08,
                usage: 0x15,
            },
            SetupInputAction::Enter,
        ]),
        Ok(())
    );
    for _ in 0..4 {
        assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    }

    // Then: the shortcut report carries the left-GUI modifier and `r` usage.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0x08, 0, 0x15, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x20, 8).unwrap(), [0; 8]);
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER + 0x40, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_BUFFER + 0x60, 8).unwrap(), [0; 8]);
}

#[test]
fn slot1_dci3_failed_transfer_preserves_queued_boot_key_report() {
    // Given: a queued Space report and a first DCI3 Normal TRB with an unmapped buffer.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_INVALID_BUFFER, true);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());

    // When: the invalid transfer fails, then the same TRB is corrected and polled twice.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0xaa; 8]);
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);
    assert_eq!(xhci.slot1_dci3_dequeue, DCI3_RING);
    assert!(!xhci.queue_boot_keyboard_space());
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_BUFFER, true);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the preserved Space report is emitted before the release report.
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event(&mem, EVENT_RING + (TRB_SIZE * 2), DCI3_RING + TRB_SIZE);
}

#[test]
fn host_controller_reset_clears_captured_dci3_state() {
    // Given: Configure Endpoint installed slot 1 HID interrupt IN DCI3, then HCRST reset runs.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    xhci.mmio_write(0x40, 4, u64::from(USB_CMD_HCRST));

    // When: the guest rings the stale slot 1 DCI3 doorbell.
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: no stale DCI3 transfer event is posted.
    assert_eq!(mem.read_u64(EVENT_RING + TRB_SIZE), 0);

    // When: DCI3 is configured again after reset and the guest rings the doorbell.
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));

    // Then: the pending key was cleared by HCRST, so the next report is still no-key.
    assert_eq!(mem.read_bytes(DCI3_BUFFER, 8).unwrap(), [0; 8]);
}

#[test]
fn slot1_dci3_readdress_preserves_setup_input_endpoint_until_reconfigured() {
    // Given: an early DCI3 Configure Endpoint installed a ring and emitted the boot-key pair.
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(0x9000);
    setup_configure_endpoint_command(&mut xhci, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_WRAP_BUFFER, true);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert!(xhci.queue_boot_keyboard_space());
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(DCI3), &mut mem));
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    assert!(mem.write_bytes(DCI3_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_WRAP_BUFFER, &[0xbb; 8]));

    // When: Windows re-addresses slot 1 via EP0 only, then setup input queues reports.
    setup_address_device_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(xhci.slot1_ep0_dequeue, READDRESS_EP0_RING);
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Enter]),
        Ok(())
    );

    // Then: a non-reset readdress keeps the configured DCI3 endpoint usable.
    assert!(xhci.process_queued_dci3_input(&mut mem));
    assert!(xhci.process_queued_dci3_input(&mut mem));
    assert_eq!(
        mem.read_bytes(DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(mem.read_bytes(DCI3_WRAP_BUFFER, 8).unwrap(), [0; 8]);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.queued_reports, 4);
    assert_eq!(stats.emitted_key_reports, 2);
    assert_eq!(stats.emitted_release_reports, 2);

    // When: a later Configure Endpoint installs a new DCI3 ring.
    setup_reconfigure_endpoint_command(&mut xhci, &mut mem);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(
        xhci.queue_setup_input_actions(&[SetupInputAction::Space]),
        Ok(())
    );
    assert!(xhci.process_queued_dci3_input(&mut mem));

    // Then: subsequent setup-input reports drain to the new ring instead.
    assert_eq!(
        mem.read_bytes(NEW_DCI3_BUFFER, 8).unwrap(),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_success_dci3_transfer_event(&mem, EVENT_RING + TRB_SIZE, NEW_DCI3_RING);
    let stats = xhci.setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 3);
    assert_eq!(stats.emitted_release_reports, 2);
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

fn setup_reconfigure_endpoint_command(xhci: &mut XhciController, mem: &mut TestRam) {
    setup_command_rings_with_parameter(
        xhci,
        mem,
        RECONFIGURE_INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    mem.write_u64(DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    mem.write_u32(
        RECONFIGURE_INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG,
    );
    mem.write_u32(
        RECONFIGURE_INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    mem.write_u64(
        RECONFIGURE_INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        NEW_DCI3_RING | TRB_CYCLE,
    );
    mem.write_u32(
        RECONFIGURE_INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_dci3_normal_trb(mem, NEW_DCI3_RING, NEW_DCI3_BUFFER, true);
    assert!(mem.write_bytes(NEW_DCI3_BUFFER, &[0xcc; 8]));
}
