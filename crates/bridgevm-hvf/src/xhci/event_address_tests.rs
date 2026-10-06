use super::test_support::{
    assert_success_completion, command_control, setup_event_ring, TestRam, CMD_RING, DOORBELL_BASE,
    EVENT_RING, TRB_SIZE,
};
use super::XhciController;
use crate::fwcfg::GuestMemoryMut;

const ERST: u64 = 0x2000;
const RAM_BYTES: usize = 0x5000;
const NO_OP: u32 = 23;

fn command_ring() -> (XhciController, TestRam) {
    let mut xhci = XhciController::new();
    let mut mem = TestRam::new(RAM_BYTES);
    setup_event_ring(&mut xhci, &mut mem);
    for index in 0..32 {
        mem.write_u32(CMD_RING + index * TRB_SIZE + 12, command_control(NO_OP, 0));
    }
    xhci.mmio_write(0x40, 4, 1);
    (xhci, mem)
}

fn reject_and_retry(segment_base: u64, acknowledge: bool) {
    let (mut xhci, mut mem) = command_ring();
    for index in 0..4 {
        assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
        assert_success_completion(
            &mem,
            EVENT_RING + index * TRB_SIZE,
            CMD_RING + index * TRB_SIZE,
            0,
        );
    }
    if acknowledge {
        xhci.mmio_write(0x1038, 8, (EVENT_RING + 4 * TRB_SIZE) | 8);
    }
    let command_before = xhci.mmio_read(0x58, 8);
    let interrupt_before = [
        xhci.mmio_read(0x1020, 4),
        xhci.mmio_read(0x44, 4),
        xhci.mmio_read(0x1038, 8),
    ];
    let stats_before = xhci.event_lifecycle_stats();
    // The descriptor remains guest-owned RAM and is read anew on every post.
    mem.write_u64(ERST, segment_base);
    let ram_before = mem.read_bytes(0, RAM_BYTES).unwrap();
    assert!(!xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_eq!(mem.read_bytes(0, RAM_BYTES).unwrap(), ram_before);
    assert_eq!(xhci.mmio_read(0x58, 8), command_before);
    assert_eq!(
        [
            xhci.mmio_read(0x1020, 4),
            xhci.mmio_read(0x44, 4),
            xhci.mmio_read(0x1038, 8)
        ],
        interrupt_before
    );
    let stats_after = xhci.event_lifecycle_stats();
    assert_eq!(
        stats_after.event_post_attempts,
        stats_before.event_post_attempts + 1
    );
    assert_eq!(
        stats_after.event_post_failures,
        stats_before.event_post_failures + 1
    );
    assert_eq!(
        stats_after.event_post_successes,
        stats_before.event_post_successes
    );
    assert_eq!(stats_after.last_event_gpa, stats_before.last_event_gpa);
    assert_eq!(
        stats_after.last_event_parameter,
        stats_before.last_event_parameter
    );
    mem.write_u64(ERST, EVENT_RING);
    assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
    assert_success_completion(&mem, EVENT_RING + 4 * TRB_SIZE, CMD_RING + 4 * TRB_SIZE, 0);
    assert_eq!(xhci.mmio_read(0x58, 8), CMD_RING + 5 * TRB_SIZE + 1);
}

#[test]
fn overflowing_event_address_rejects_without_creating_interrupt() {
    reject_and_retry(!63, true);
}

#[test]
fn overflowing_event_address_preserves_pending_interrupt() {
    reject_and_retry(!63, false);
}

#[test]
fn nonoverflowing_unmapped_event_address_keeps_existing_retry_behavior() {
    reject_and_retry(0x10_0000, true);
}

#[test]
fn mapped_event_ring_wraps_and_toggles_cycle_after_guest_consumption() {
    let (mut xhci, mut mem) = command_ring();
    for index in 0..32 {
        let event = EVENT_RING + (index % 16) * TRB_SIZE;
        assert!(xhci.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut mem));
        assert_eq!(mem.read_u64(event), CMD_RING + index * TRB_SIZE);
        assert_eq!(mem.read_u32(event + 8) >> 24, 1);
        assert_eq!(mem.read_u32(event + 12), (33 << 10) | u32::from(index < 16));
        xhci.mmio_write(0x1038, 8, (EVENT_RING + ((index + 1) % 16) * TRB_SIZE) | 8);
    }
    assert_eq!(
        xhci.event_lifecycle_stats().command_completion_event_posts,
        32
    );
    assert_eq!(xhci.event_lifecycle_stats().event_post_failures, 0);
}
