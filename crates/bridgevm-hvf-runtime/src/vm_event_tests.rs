use super::*;

#[test]
fn current_generation_events_drain_in_publish_order() {
    let generation = ResetGeneration::new();
    let queue = VmEventQueue::new();
    queue.publish(generation.stamp(), VmEvent::GuestRequestedReset);
    queue.publish(
        generation.stamp(),
        VmEvent::AgentConsoleLine("hello".into()),
    );
    let drained = queue.drain(&generation);
    assert_eq!(
        drained.events,
        vec![
            VmEvent::GuestRequestedReset,
            VmEvent::AgentConsoleLine("hello".into()),
        ]
    );
    assert_eq!(drained.stale_discarded, 0);
}

#[test]
fn events_stamped_before_a_reset_are_discarded_and_counted() {
    let generation = ResetGeneration::new();
    let queue = VmEventQueue::new();
    let old = generation.stamp();
    generation.advance();
    // Published AFTER the reset but observed before it: the stamp decides.
    queue.publish(old, VmEvent::GuestRequestedReset);
    queue.publish(generation.stamp(), VmEvent::GuestRequestedPowerOff);
    let drained = queue.drain(&generation);
    assert_eq!(drained.events, vec![VmEvent::GuestRequestedPowerOff]);
    assert_eq!(drained.stale_discarded, 1);
}

#[test]
fn a_reset_between_publish_and_drain_stales_the_whole_queue() {
    let generation = ResetGeneration::new();
    let queue = VmEventQueue::new();
    queue.publish(
        generation.stamp(),
        VmEvent::BootProgressStall("stall".into()),
    );
    generation.advance();
    let drained = queue.drain(&generation);
    assert!(drained.events.is_empty());
    assert_eq!(drained.stale_discarded, 1);
}

#[test]
fn drain_empties_the_queue() {
    let generation = ResetGeneration::new();
    let queue = VmEventQueue::new();
    queue.publish(generation.stamp(), VmEvent::GuestRequestedReset);
    queue.drain(&generation);
    let second = queue.drain(&generation);
    assert!(second.events.is_empty());
    assert_eq!(second.stale_discarded, 0);
}
