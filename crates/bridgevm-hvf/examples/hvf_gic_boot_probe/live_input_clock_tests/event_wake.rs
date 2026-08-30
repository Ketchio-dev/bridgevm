use super::super::{InputControlFile, LiveInputController};
use crate::xhci_hid_input::test_support::new_platform_and_ram;
use std::collections::VecDeque;
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn vnode_event_wake_bypasses_the_periodic_poll_deadline() {
    let (mut platform, mut mem) = new_platform_and_ram();
    let path = std::env::temp_dir().join(format!(
        "bridgevm-live-event-wake-{}.ctl",
        std::process::id()
    ));
    fs::write(&path, b"RESIZE 800x600\n").unwrap();
    let now = Instant::now();
    let mut input = LiveInputController {
        source: Some(InputControlFile::from_path(path.clone())),
        offset: 0,
        partial: String::new(),
        pending: VecDeque::new(),
        accepted_pointer_moves: 0,
        next_poll: now + Duration::from_secs(60),
    };
    assert!(!input.poll_due(now));

    input.tick(&mut platform, &mut mem, now, true);

    assert!(input.offset > 0);
    assert!(input.pending.is_empty());
    fs::remove_file(path).unwrap();
}
