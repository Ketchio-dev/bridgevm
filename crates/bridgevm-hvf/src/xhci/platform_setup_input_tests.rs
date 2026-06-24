use super::platform_test_support::*;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::{FlatGuestRam, VirtPlatform, XhciHidBootKeyQueueError};
use crate::xhci::{SetupInputAction, XhciSetupInputQueueError};

#[test]
fn platform_queued_hid_boot_key_reaches_dci3_over_pcie_bar0() {
    // Given: the platform owns an xHCI controller configured through PCIe BAR0.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));

    // When: the platform route queues Space and the guest polls DCI3 twice.
    assert!(platform.queue_xhci_hid_boot_key_usage(0x2c).is_ok());
    assert_eq!(
        platform
            .xhci_hid_boot_key_report_stats()
            .queued_space_reports,
        1
    );
    ring_dci3_doorbell(&mut platform, &mut mem);
    ring_dci3_doorbell(&mut platform, &mut mem);

    // Then: the BAR0 route emits one Space report and one release report.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 2),
        DCI3_RING + TRB_SIZE,
    );
}

#[test]
fn platform_setup_input_action_queue_reaches_dci3_over_pcie_bar0() {
    // Given: the platform owns an xHCI controller configured through PCIe BAR0.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..6 {
        write_dci3_normal_trb(
            &mut mem,
            DCI3_RING + (TRB_SIZE * index),
            DCI3_KEY_BUFFER + (0x20 * index),
        );
        assert!(mem.write_bytes(DCI3_KEY_BUFFER + (0x20 * index), &[0xaa; 8]));
    }

    // When: the platform queues the minimal typed setup sequence and DCI3 is polled.
    assert_eq!(
        platform.queue_xhci_setup_input_actions(&[
            SetupInputAction::Tab,
            SetupInputAction::Enter,
            SetupInputAction::Space,
        ]),
        Ok(())
    );
    for _ in 0..6 {
        ring_dci3_doorbell(&mut platform, &mut mem);
    }

    // Then: each action emits one key report and one release report in order.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x2b, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0x20, 8), [0; 8]);
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x40, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0x60, 8), [0; 8]);
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x80, 8),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0xa0, 8), [0; 8]);
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 3);
    assert_eq!(stats.queued_reports, 6);
    assert_eq!(stats.emitted_key_reports, 3);
    assert_eq!(stats.emitted_release_reports, 3);
}

#[test]
fn platform_rejects_unsupported_hid_boot_key_usage() {
    // Given: the platform has a ready xHCI DCI3 interrupt-IN ring.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));

    // When: an unsupported HID Keyboard/Keypad usage is requested.
    assert_eq!(
        platform.queue_xhci_hid_boot_key_usage(0x04),
        Err(XhciHidBootKeyQueueError::UnsupportedUsage { usage: 0x04 })
    );
    assert_eq!(
        platform
            .xhci_hid_boot_key_report_stats()
            .unsupported_usage_rejections,
        1
    );
    ring_dci3_doorbell(&mut platform, &mut mem);

    // Then: the queue is rejected and DCI3 remains in the existing no-key state.
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
}

#[test]
fn platform_rejects_empty_setup_input_without_stale_report() {
    // Given: the platform has a ready xHCI DCI3 interrupt-IN ring.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));

    // When: an empty typed setup-input sequence is rejected before DCI3 is polled.
    assert_eq!(
        platform.queue_xhci_setup_input_actions(&[]),
        Err(XhciSetupInputQueueError::EmptySequence)
    );
    ring_dci3_doorbell(&mut platform, &mut mem);

    // Then: the rejected queue did not leave a stale key report behind.
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER, 8), [0; 8]);
    assert_eq!(
        platform
            .xhci_setup_input_report_stats()
            .empty_sequence_rejections,
        1
    );
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + TRB_SIZE, DCI3_RING);
}

fn configure_dci3_interrupt_in_over_bar0(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    write_event_ring_table(mem);
    write_u64(mem, DCBAA + (u64::from(ENABLE_SLOT_ID) * 8), OUTPUT_CONTEXT);
    write_u32(
        mem,
        INPUT_CONTEXT + INPUT_CONTROL_ADD_CONTEXT_OFFSET,
        DCI3_ADD_CONTEXT_FLAG,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD1_OFFSET,
        DCI3_DWORD1,
    );
    write_u64(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET,
        DCI3_RING | TRB_CYCLE,
    );
    write_u32(
        mem,
        INPUT_CONTEXT + DCI3_INPUT_CONTEXT_OFFSET + EP_CONTEXT_DWORD4_OFFSET,
        DCI3_DWORD4,
    );
    write_command_trb_with_parameter(
        mem,
        INPUT_CONTEXT,
        command_control(TRB_TYPE_CONFIGURE_ENDPOINT, ENABLE_SLOT_ID),
    );
    for (offset, size, value) in [
        (0x58, 8, COMMAND_RING | TRB_CYCLE),
        (0x70, 8, DCBAA),
        (0x78, 4, 1),
        (0x1020, 4, 2),
        (0x1028, 4, 1),
        (0x1030, 8, ERST),
        (0x1038, 8, EVENT_RING),
        (0x2000, 4, 0),
    ] {
        write_xhci_bar0(
            platform,
            mem,
            BarWrite {
                offset,
                size,
                value,
            },
        );
    }
    assert_success_completion(mem, ENABLE_SLOT_ID);
}

fn write_dci3_normal_trb(mem: &mut FlatGuestRam, trb_gpa: u64, buffer_gpa: u64) {
    write_u64(mem, trb_gpa, buffer_gpa);
    write_u32(mem, trb_gpa + 8, 8);
    write_u32(mem, trb_gpa + 12, (TRB_TYPE_NORMAL << 10) | 1);
}

fn ring_dci3_doorbell(platform: &mut VirtPlatform, mem: &mut FlatGuestRam) {
    write_xhci_bar0(
        platform,
        mem,
        BarWrite {
            offset: 0x2004,
            size: 4,
            value: u64::from(DCI3),
        },
    );
}

fn assert_success_dci3_transfer_event_for_trb(mem: &FlatGuestRam, event_gpa: u64, trb_gpa: u64) {
    assert_eq!(read_u64(mem, event_gpa), trb_gpa);
    assert_eq!(read_u32(mem, event_gpa + 8) >> 24, COMPLETION_CODE_SUCCESS);
    let control = read_u32(mem, event_gpa + 12);
    assert_eq!((control >> 10) & 0x3f, TRB_TYPE_TRANSFER_EVENT);
    assert_eq!((control >> 16) & 0x1f, DCI3);
    assert_eq!(control >> 24, ENABLE_SLOT_ID);
    assert_eq!(control & 1, 1);
}

fn write_u32(mem: &mut FlatGuestRam, gpa: u64, value: u32) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}

fn write_u64(mem: &mut FlatGuestRam, gpa: u64, value: u64) {
    assert!(mem.write_bytes(gpa, &value.to_le_bytes()));
}
