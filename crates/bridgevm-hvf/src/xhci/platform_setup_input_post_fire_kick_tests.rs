use super::platform_setup_input_support::*;
use super::platform_test_support::*;
use crate::fwcfg::GuestMemoryMut;
use crate::xhci::SetupInputAction;

#[test]
fn platform_setup_input_post_fire_kicks_reposted_dci3_poll_after_boot_key() {
    // Given: the boot key consumed the initial DCI3 key/release polls, then the
    // guest parked the DCI3 ring until setup input fires.
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
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER, 8),
        [0, 0, 0x2c, 0, 0, 0, 0, 0]
    );
    assert_eq!(read_bytes(&mem, DCI3_RELEASE_BUFFER, 8), [0; 8]);
    assert!(mem.write_bytes(DCI3_RING, &[0; 16]));
    assert!(mem.write_bytes(DCI3_RING + TRB_SIZE, &[0; 16]));

    // When: setup input fires while DCI3 is parked, then the guest reposts the
    // next interrupt-IN poll without ringing another DCI3 doorbell.
    assert_eq!(
        platform.queue_xhci_setup_input_actions_with_mem(&[SetupInputAction::Enter], &mut mem),
        Ok(())
    );
    write_dci3_normal_trb(&mut mem, DCI3_RING, DCI3_KEY_BUFFER + 0x40);
    assert!(mem.write_bytes(DCI3_KEY_BUFFER + 0x40, &[0xcc; 8]));
    acknowledge_event_ring_dequeue(&mut platform, &mut mem, 3);

    // Then: the post-fire kick path drains the reposted DCI3 poll immediately,
    // proving setup input is no longer stuck behind the earlier boot key.
    assert_eq!(
        read_bytes(&mem, DCI3_KEY_BUFFER + 0x40, 8),
        [0, 0, 0x28, 0, 0, 0, 0, 0]
    );
    assert_success_dci3_transfer_event_for_trb(&mem, EVENT_RING + (TRB_SIZE * 3), DCI3_RING);
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!(stats.queued_actions, 2);
    assert_eq!(stats.queued_reports, 4);
    assert_eq!(stats.emitted_key_reports, 2);
    assert_eq!(stats.emitted_release_reports, 1);
}
