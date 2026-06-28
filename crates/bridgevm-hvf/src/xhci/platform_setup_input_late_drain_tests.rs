use super::platform_setup_input_support::*;
use super::platform_test_support::*;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::SetupInputAction;

#[test]
fn platform_delayed_setup_input_rearms_reusable_dci3_ring_without_new_doorbell() {
    // Given: live DCI3 polling has advanced past a small reusable interrupt IN ring.
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
        ring_dci3_doorbell(&mut platform, &mut mem);
        acknowledge_event_ring_dequeue(&mut platform, &mut mem, index + 2);
    }
    assert!(mem.write_bytes(DCI3_KEY_BUFFER, &[0xcc; 8]));
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0x20, &[0xdd; 8]));

    // When: delayed setup input queues more reports than the reusable ring holds.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(
            &[
                SetupInputAction::Enter,
                SetupInputAction::Tab,
                SetupInputAction::Space,
            ],
            &mut mem
        ),
        Ok(())
    );

    // Then: queue-time draining rearms to the existing reusable DCI3 TRBs immediately.
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 5), DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 6),
        DCI3_RING + TRB_SIZE,
    );
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x40, 8),
        [0, 0, 0x2b, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0x60, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 7),
        DCI3_RING + (TRB_SIZE * 2),
    );
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 8),
        DCI3_RING + (TRB_SIZE * 3),
    );
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0x20, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 9), DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(
        &mem,
        EVENT_RING + (TRB_SIZE * 10),
        DCI3_RING + TRB_SIZE,
    );
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 3);
    assert_eq!(stats.emitted_release_reports, 3);
}

#[test]
fn platform_delayed_setup_input_drains_reposted_dci3_trbs_after_initial_boot_key() {
    // Given: the installer boot key used the first two DCI3 TRBs before setup input fires.
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
    assert!(mem.write_bytes(DCI3_RING, &[0; 16]));
    assert!(mem.write_bytes(DCI3_RING + TRB_SIZE, &[0; 16]));

    // When: delayed setup input queues Enter, then Windows reposts DCI3 slot 0 twice.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(&[SetupInputAction::Enter], &mut mem),
        Ok(())
    );
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER + 0x40);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0x40, &[0xcc; 8]));
    ring_dci3_doorbell(&mut platform, &mut mem);
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 4);
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER + 0x60);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0x60, &[0xdd; 8]));
    ring_dci3_doorbell(&mut platform, &mut mem);
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 5);

    // Then: reposted DCI3 TRBs drain the queued setup-input key and release.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x40, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0x60, 8), [0; 8]);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 3), DCI3_RING);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 4), DCI3_RING);
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 2);
    assert_eq!(stats.queued_reports, 4);
    assert_eq!(stats.emitted_key_reports, 2);
    assert_eq!(stats.emitted_release_reports, 2);
}

#[test]
fn platform_late_dci3_trbs_drain_on_runtime_event_ack_without_second_doorbell() {
    // Given: setup input needs six DCI3 polls, but the guest has posted only two so far.
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..2 {
        write_dci3_normal_trb(
            &mut mem,
            DCI3_RING + (TRB_SIZE * index),
            DCI3_KEY_BUFFER + (0x20 * index),
        );
        assert!(mem.write_bytes(DCI3_KEY_BUFFER + (0x20 * index), &[0xaa; 8]));
    }

    // When: setup input is queued, then later DCI3 TRBs appear without another DCI3 doorbell.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(
            &[
                SetupInputAction::Tab,
                SetupInputAction::Enter,
                SetupInputAction::Space,
            ],
            &mut mem
        ),
        Ok(())
    );
    for index in 2..6 {
        write_dci3_normal_trb(
            &mut mem,
            DCI3_RING + (TRB_SIZE * index),
            DCI3_KEY_BUFFER + (0x20 * index),
        );
        assert!(mem.write_bytes(DCI3_KEY_BUFFER + (0x20 * index), &[0xcc; 8]));
        acknowledge_event_ring_dequeue(&mut platform, &mut mem, index + 1);
    }

    // Then: each late TRB drains from realistic runtime ERDP/EHB writes alone.
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
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 3), DCI3_RING + 0x20);
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 6), DCI3_RING + 0x50);

    // Then: once the setup-input queue is empty, a later runtime write does not post no-key.
    write_dci3_normal_trb(&mut mem, DCI3_RING + (TRB_SIZE * 6), DCI3_KEY_BUFFER + 0xc0);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0xc0, &[0xdd; 8]));
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 7);
    assert_eq!(read_bytes(&mem, DCI3_KEY_BUFFER + 0xc0, 8), [0xdd; 8]);
    assert_eq!(read_u64(&mem, EVENT_RING + (TRB_SIZE * 7)), 0);

    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.emitted_key_reports, 3);
    assert_eq!(stats.emitted_release_reports, 3);
}
