use super::platform_setup_input_support::*;
use super::platform_test_support::*;
use crate::fwcfg::GuestMemoryMut;
use crate::platform_virt::XhciHidBootKeyQueueError;
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
fn platform_setup_input_action_queue_delivers_pending_dci3_without_doorbell() {
    // Given: the guest already posted two DCI3 interrupt-IN TRBs and is waiting.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER);
    write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE, DCI3_RELEASE_BUFFER);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xaa; 8]));
    assert!(mem.write_bytes(DCI3_RELEASE_BUFFER, &[0xbb; 8]));

    // When: setup input is queued after the TRBs are already available.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(&[SetupInputAction::Enter], &mut mem),
        Ok(())
    );

    // Then: the queued Enter press and release are delivered without another doorbell.
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
    assert_eq!(stats.emitted_key_reports, 1);
    assert_eq!(stats.emitted_release_reports, 1);
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

    // Then: the usage was rejected, so nothing is queued and the idle DCI3 poll
    // NAKs — the buffer is untouched and no transfer event is posted.
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER, 8), [0xaa; 8]);
    assert_eq!(read_bytes(&mem, EVENT_RING + TRB_SIZE, 8), [0; 8]);
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

    // Then: the rejected queue left nothing to deliver, so the idle DCI3 poll
    // NAKs without writing a stale report or posting a transfer event.
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER, 8), [0xaa; 8]);
    assert_eq!(read_bytes(&mem, EVENT_RING + TRB_SIZE, 8), [0; 8]);
    assert_eq!(
        platform
            .xhci_setup_input_report_stats()
            .empty_sequence_rejections,
        1
    );
}

#[test]
fn paced_setup_input_drain_releases_one_report_per_host_interval() {
    use std::time::{Duration, Instant};

    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    // Arm four ready interrupt-IN TDs so the guest side never limits draining.
    for index in 0..4 {
        write_dci3_normal_trb(
            &mut mem,
            DCI3_RING + (TRB_SIZE * index),
            DCI3_KEY_BUFFER + (0x20 * index),
        );
        assert!(mem.write_bytes(DCI3_KEY_BUFFER + (0x20 * index), &[0xaa; 8]));
    }
    platform.set_xhci_report_interval(Duration::from_millis(30));
    let base = Instant::now();
    platform.set_host_now(base);

    // Two keys = four reports with four TDs ready; unpaced this drains in one call.
    platform
        .queue_xhci_setup_input_actions(&[SetupInputAction::Tab, SetupInputAction::Enter])
        .unwrap();

    let emitted = |platform: &crate::platform_virt::VirtPlatform| {
        let stats = platform.xhci_setup_input_report_stats();
        stats.emitted_key_reports + stats.emitted_release_reports
    };

    // At `base` only the first report is released; repeat drains at the same host
    // time release nothing more (pacing gate holds).
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(emitted(&platform), 1);
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(emitted(&platform), 1);

    // A step short of the interval still releases nothing.
    platform.set_host_now(base + Duration::from_millis(29));
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(emitted(&platform), 1);

    // Each full 30ms host-time step releases exactly one more report.
    platform.set_host_now(base + Duration::from_millis(30));
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(emitted(&platform), 2);

    platform.set_host_now(base + Duration::from_millis(60));
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(emitted(&platform), 3);

    platform.set_host_now(base + Duration::from_millis(90));
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(emitted(&platform), 4);
}

#[test]
fn zero_interval_keeps_the_burst_drain_even_with_a_host_clock() {
    use std::time::{Duration, Instant};

    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..4 {
        write_dci3_normal_trb(
            &mut mem,
            DCI3_RING + (TRB_SIZE * index),
            DCI3_KEY_BUFFER + (0x20 * index),
        );
        assert!(mem.write_bytes(DCI3_KEY_BUFFER + (0x20 * index), &[0xaa; 8]));
    }
    // Interval left at the default (zero) even though a host clock is present.
    platform.set_xhci_report_interval(Duration::ZERO);
    platform.set_host_now(Instant::now());
    platform
        .queue_xhci_setup_input_actions(&[SetupInputAction::Tab, SetupInputAction::Enter])
        .unwrap();

    // One drain releases all four reports — old, unpaced behavior preserved.
    platform.drain_xhci_setup_input_reports(&mut mem);
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports + stats.emitted_release_reports, 4);
}
