//! Held keys (`KEY down:<key>` / `KEY up:<key>`): one report per change, and every report carries
//! the keys still held, so the guest sees one continuous press.

use super::{BootKeyboardReportQueue, SetupInputAction, XhciSetupInputQueueError};

const LEFT: SetupInputAction = SetupInputAction::Key {
    name: "left",
    modifier: 0,
    usage: 0x50,
};

fn key(usage: u8, modifier: u8) -> SetupInputAction {
    SetupInputAction::Key {
        name: "chord",
        modifier,
        usage,
    }
}

/// The queued reports' bytes, emptying the queue as the guest would by reading them.
fn drain(queue: &mut BootKeyboardReportQueue) -> Vec<[u8; 8]> {
    let mut reports = Vec::new();
    while let Some(report) = queue.peek() {
        reports.push(report.bytes());
        queue.pop_front();
    }
    reports
}

#[test]
fn a_held_key_stays_in_every_report_until_its_release() {
    let mut queue = BootKeyboardReportQueue::default();
    assert_eq!(queue.queue_hold(LEFT, true), Ok(1));
    assert_eq!(drain(&mut queue), [[0, 0, 0x50, 0, 0, 0, 0, 0]]);
    // A tap while it is held (typed text): the key report and its release keep Left down.
    assert_eq!(queue.queue_actions(&[key(0x04, 0x02)]), Ok(2));
    assert_eq!(
        drain(&mut queue),
        [
            [0x02, 0, 0x50, 0x04, 0, 0, 0, 0],
            [0, 0, 0x50, 0, 0, 0, 0, 0]
        ]
    );
    assert_eq!(queue.queue_hold(key(0x4f, 0x01), true), Ok(1));
    assert_eq!(drain(&mut queue), [[0x01, 0, 0x50, 0x4f, 0, 0, 0, 0]]);
    assert_eq!(queue.queue_hold(LEFT, false), Ok(1));
    assert_eq!(drain(&mut queue), [[0x01, 0, 0x4f, 0, 0, 0, 0, 0]]);
    assert_eq!(queue.queue_hold(key(0x4f, 0x01), false), Ok(1));
    assert_eq!(drain(&mut queue), [[0; 8]]);
}

#[test]
fn a_repeated_press_or_a_release_without_a_press_changes_nothing() {
    let mut queue = BootKeyboardReportQueue::default();
    assert_eq!(queue.queue_hold(LEFT, false), Ok(0), "up without down");
    assert_eq!(queue.queue_hold(LEFT, true), Ok(1));
    assert_eq!(
        queue.queue_hold(LEFT, true),
        Err(XhciSetupInputQueueError::Busy),
        "one command at a time, as for taps"
    );
    drain(&mut queue);
    assert_eq!(queue.queue_hold(LEFT, true), Ok(0), "already down");
    assert!(drain(&mut queue).is_empty());
    assert_eq!(queue.queue_hold(LEFT, false), Ok(1));
    assert_eq!(drain(&mut queue), [[0; 8]]);
    assert_eq!(queue.queue_hold(LEFT, false), Ok(0), "already up");
}

#[test]
fn six_keys_are_held_at_most_and_a_controller_reset_releases_them() {
    let mut queue = BootKeyboardReportQueue::default();
    for usage in 0x3a..0x40 {
        assert_eq!(queue.queue_hold(key(usage, 0), true), Ok(1));
        drain(&mut queue);
    }
    assert_eq!(
        queue.queue_hold(key(0x40, 0), true),
        Err(XhciSetupInputQueueError::TooManyActions {
            requested: 7,
            max: 6
        })
    );
    assert!(drain(&mut queue).is_empty());
    // A tap with every slot held still reaches the guest, in the last slot.
    assert_eq!(queue.queue_actions(&[key(0x28, 0)]), Ok(2));
    assert_eq!(
        drain(&mut queue),
        [
            [0, 0, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x28],
            [0, 0, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x3f]
        ]
    );
    queue.clear();
    assert_eq!(queue.queue_actions(&[key(0x28, 0)]), Ok(2));
    assert_eq!(
        drain(&mut queue),
        [[0, 0, 0x28, 0, 0, 0, 0, 0], [0; 8]],
        "nothing held after a reset"
    );
}
