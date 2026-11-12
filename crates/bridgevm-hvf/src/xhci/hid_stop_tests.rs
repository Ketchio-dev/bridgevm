use super::configure_endpoint_pointer_tests::{
    setup_configure_endpoint_with_pointer, DCI5_BUFFER, DCI5_RING,
};
use super::configure_endpoint_tests::{
    write_dci3_normal_trb, DCI3_BUFFER, DCI3_RING, OUTPUT_CONTEXT,
};
use super::test_support::{
    command_control, TestRam, CMD_RING, DOORBELL_BASE, EVENT_RING, TRB_SIZE,
};
use super::*;
use crate::fwcfg::GuestMemoryMut;

struct ObserveCompletion {
    ram: TestRam,
    context: u64,
    state_at_completion: Option<u32>,
}
impl GuestMemoryMut for ObserveCompletion {
    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.ram.read_bytes(gpa, len)
    }
    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        self.ram.read_into(gpa, dst)
    }
    fn write_bytes(&mut self, gpa: u64, bytes: &[u8]) -> bool {
        if gpa == EVENT_RING + TRB_SIZE + 12 {
            self.state_at_completion = Some(self.ram.read_u32(self.context) & 7);
        }
        self.ram.write_bytes(gpa, bytes)
    }
}

#[test]
fn hid_stop_state_is_visible_when_completion_cycle_is_published() {
    for ep in [3, 5] {
        let (mut x, mut m, _, _, context) = setup(ep);
        m.write_u32(x.command_dequeue + 12, command_control(15, 1) | (ep << 16));
        let mut observed = ObserveCompletion {
            ram: m,
            context,
            state_at_completion: None,
        };
        assert!(x.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut observed));
        assert_eq!(observed.state_at_completion, Some(3));
    }
}

#[test]
fn hid_stop_and_restart_do_not_follow_unreadable_or_circular_guest_rings_forever() {
    for ep in [3, 5] {
        let (mut x, mut m, _, _, _) = setup(ep);
        command(&mut x, &mut m, 15, ep, 0);
        command(&mut x, &mut m, 16, ep, u64::MAX);
        assert!(!x.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(ep), &mut m));
        command(&mut x, &mut m, 15, ep, 0);
        command(&mut x, &mut m, 16, ep, 0x6601);
        m.write_u64(0x6600, 0x6600);
        m.write_u32(0x660c, (6 << 10) | 1);
        assert!(!x.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(ep), &mut m));
    }
}

fn command(x: &mut XhciController, m: &mut TestRam, kind: u32, ep: u32, parameter: u64) {
    let at = x.command_dequeue;
    m.write_u64(at, parameter);
    m.write_u32(at + 12, command_control(kind, 1) | (ep << 16));
    assert!(x.mmio_write_with_mem(DOORBELL_BASE, 4, 0, m));
}
fn setup(ep: u32) -> (XhciController, TestRam, u64, u64, u64) {
    let mut x = XhciController::new();
    let mut m = TestRam::new(0x9000);
    setup_configure_endpoint_with_pointer(&mut x, &mut m);
    assert!(x.mmio_write_with_mem(DOORBELL_BASE, 4, 0, &mut m));
    let (ring, buffer) = if ep == 3 {
        (DCI3_RING, DCI3_BUFFER)
    } else {
        (DCI5_RING, DCI5_BUFFER)
    };
    (x, m, ring, buffer, OUTPUT_CONTEXT + u64::from(ep) * 32)
}
fn queue(x: &mut XhciController, ep: u32) {
    if ep == 3 {
        assert!(x.queue_boot_keyboard_space());
    } else {
        x.queue_pointer_input_actions(&[PointerInputAction::Move(
            PointerPosition::new(12000, 20000).unwrap(),
        )])
        .unwrap();
    }
}
fn drain(x: &mut XhciController, m: &mut TestRam, ep: u32) -> bool {
    if ep == 3 {
        x.process_queued_dci3_input(m)
    } else {
        x.process_queued_dci5_pointer_input(m)
    }
}
#[test]
fn hid_stop_publishes_state_and_pending_transfer_before_command_completion() {
    for ep in [3, 5] {
        let (mut x, mut m, ring, _, context) = setup(ep);
        assert!(!x.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(ep), &mut m));
        // Chain Event Data: cancellation must still name the interrupted
        // Normal TRB rather than the cookie of an unexecuted Event Data TRB.
        m.write_u32(ring + 12, (1 << 10) | 1 | (1 << 4));
        m.write_u64(ring + TRB_SIZE, 0xabcdef);
        m.write_u32(ring + TRB_SIZE + 12, (7 << 10) | 1 | (1 << 5));
        command(&mut x, &mut m, 15, ep, 0);
        assert_eq!(m.read_u32(context) & 7, 3, "DCI{ep} must be Stopped");
        assert_eq!(m.read_u64(context + 8), ring | 1);
        let event = EVENT_RING + TRB_SIZE;
        assert_eq!(
            m.read_u64(event),
            ring,
            "stopped event names interrupted Normal TRB, not Event Data"
        );
        assert_eq!(m.read_u32(event + 8), (26 << 24) | 8);
        assert_eq!((m.read_u32(event + 12) >> 16) & 31, ep);
        assert_eq!(m.read_u32(event + 12) >> 24, 1);
        super::test_support::assert_success_completion(
            &m,
            event + TRB_SIZE,
            CMD_RING + TRB_SIZE,
            1,
        );
    }
}
#[test]
fn hid_stop_blocks_dma_until_doorbell_and_set_dequeue_resumes_at_new_ring() {
    for ep in [3, 5] {
        let (mut x, mut m, ring, buffer, context) = setup(ep);
        command(&mut x, &mut m, 15, ep, 0);
        queue(&mut x, ep);
        let events = x.event_lifecycle_stats.transfer_event_posts;
        assert_eq!(
            events, 0,
            "Stop must not manufacture an event for an unexecuted TD"
        );
        for _ in 0..3 {
            assert!(
                !drain(&mut x, &mut m, ep),
                "DCI{ep} must not drain after Stop"
            );
        }
        assert_eq!(m.read_bytes(buffer, 8).unwrap(), [0xaa; 8]);
        assert_eq!(x.event_lifecycle_stats.transfer_event_posts, events);
        let new_ring = 0x6600;
        let new_buffer = 0x6c00;
        write_dci3_normal_trb(&mut m, new_ring, new_buffer, false);
        command(&mut x, &mut m, 16, ep, new_ring);
        assert_eq!(m.read_u64(context + 8), new_ring);
        assert_eq!(m.read_u32(context) & 7, 3);
        assert!(!drain(&mut x, &mut m, ep));
        x.mmio_write_with_mem(DOORBELL_BASE + 4, 4, u64::from(ep), &mut m);
        if ep == 5 {
            assert!(drain(&mut x, &mut m, ep));
        }
        assert_eq!(m.read_u32(context) & 7, 1);
        assert_ne!(m.read_bytes(new_buffer, 6).unwrap(), [0; 6]);
        assert_eq!(m.read_bytes(buffer, 8).unwrap(), [0xaa; 8]);
        assert_eq!(m.read_u64(context + 8), new_ring + TRB_SIZE);
        assert_ne!(ring, new_ring);
    }
}
#[test]
fn hid_reset_and_set_dequeue_reject_inappropriate_states_without_stranding_command_ring() {
    for ep in [3, 5] {
        let (mut x, mut m, ring, _, context) = setup(ep);
        command(&mut x, &mut m, 16, ep, 0x6601);
        assert_eq!(m.read_u32(EVENT_RING + TRB_SIZE + 8) >> 24, 19);
        assert_eq!(m.read_u64(context + 8), ring | 1);
        command(&mut x, &mut m, 14, ep, 0);
        assert_eq!(m.read_u32(EVENT_RING + TRB_SIZE * 2 + 8) >> 24, 19);
        command(&mut x, &mut m, 15, ep, 0);
        command(&mut x, &mut m, 14, ep, 0);
        assert_eq!(m.read_u32(EVENT_RING + TRB_SIZE * 4 + 8) >> 24, 19);
        assert_eq!(m.read_u32(context) & 7, 3);
        // Halted recovery leaves Stopped, with the same dequeue, until a bell.
        x.hid_endpoints[XhciController::hid_index(ep).unwrap()].state = 2;
        command(&mut x, &mut m, 14, ep, 0);
        assert_eq!(m.read_u32(EVENT_RING + TRB_SIZE * 5 + 8) >> 24, 1);
        assert_eq!(m.read_u32(context) & 7, 3);
        assert_eq!(m.read_u64(context + 8), ring | 1);
        assert!(!drain(&mut x, &mut m, ep));
    }
}
