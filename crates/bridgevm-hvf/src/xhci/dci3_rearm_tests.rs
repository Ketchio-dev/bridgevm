use super::dci3_rearm::{Dci3RearmPolicy, Dci3RearmResult};
use super::test_support::TestRam;
use super::*;

const CURRENT_DCI3_RING: u64 = 0x6000;
const STALE_DCI3_RING: u64 = 0x7000;
const STALE_DCI3_DEQUEUE: u64 = STALE_DCI3_RING + 0x10;

#[test]
fn reusable_dci3_rearm_rejects_last_dequeue_from_stale_ring_base() {
    // Given: the current DCI3 ring base is empty/unsupported while remembered
    // live dequeue state came from an older Configure Endpoint ring.
    let mut xhci = XhciController::new();
    let mem = TestRam::new(0x9000);
    xhci.slot1_dci3_ring_base = CURRENT_DCI3_RING;
    xhci.slot1_dci3_dequeue = CURRENT_DCI3_RING;
    xhci.slot1_dci3_dcs = true;
    xhci.slot1_dci3_two_entry_queue_rearm = true;
    xhci.slot1_dci3_last_reusable = true;
    xhci.slot1_dci3_last_ring_base = STALE_DCI3_RING;
    xhci.slot1_dci3_last_dequeue = STALE_DCI3_DEQUEUE;
    xhci.slot1_dci3_last_dcs = true;
    xhci.queue_setup_input_actions(&[SetupInputAction::Space])
        .unwrap();

    // When: queued setup-input drain tries the reusable two-entry rearm path.
    let result =
        xhci.rearm_slot1_dci3_to_ring_base_if_queued(&mem, Dci3RearmPolicy::ReusableQueueDrain);

    // Then: stale-ring provenance is rejected instead of rearming to an
    // unrelated old dequeue pointer.
    assert!(matches!(
        result,
        Dci3RearmResult::Refused("rearm_refused_unsupported_ring_base_trb_type")
    ));
    assert_eq!(xhci.slot1_dci3_dequeue, CURRENT_DCI3_RING);
    assert_eq!(xhci.slot1_dci3_ring_base, CURRENT_DCI3_RING);
}
