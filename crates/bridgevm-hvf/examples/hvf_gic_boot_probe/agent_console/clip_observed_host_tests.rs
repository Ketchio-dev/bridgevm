//! Cross-channel ordering: polling a Mac copy must not make an older guest
//! reply eligible to overwrite it before the service consumes its event.

use super::*;
use crate::agent_console::host_pasteboard::PasteboardBackend;

struct MemoryPasteboard {
    text: String,
    count: i64,
}

impl PasteboardBackend for MemoryPasteboard {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        Some(self.text.clone())
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.text = text.to_owned();
        self.count += 1;
        true
    }
}

fn observed_mac_copy_survives_guest_reply(reply_before_copy: bool) {
    let mut rig = ClipRig::synced("baseline");
    let mut mac = MemoryPasteboard {
        text: "baseline".into(),
        count: 1,
    };
    assert!(rig.pasteboard.poll(&mut mac));
    rig.guest = "guest-before-mac".into();
    rig.enqueue();
    let poll = rig.send();
    assert_eq!(poll, "CLIPGET\n");
    if reply_before_copy {
        rig.answer(&poll);
    }

    mac.text = "mac-new".into();
    mac.count += 1;
    assert!(rig.pasteboard.poll(&mut mac));
    if !reply_before_copy {
        rig.answer(&poll);
    }
    // The real pasteboard worker can receive a guest set after observing a
    // Mac copy, while its event is still unread by the service loop.
    assert!(rig.pasteboard.apply_pending(&mut mac));
    rig.settle();
    rig.settle();
    assert_eq!(
        rig.guest, "mac-new",
        "the queued Mac event reached the guest"
    );
    assert!(
        rig.mac_sets().is_empty(),
        "no later poll repairs this divergence"
    );
    assert_eq!(
        mac.text, "mac-new",
        "observed Mac copy overwritten by stale guest reply"
    );
    rig.guest = "genuine-later-guest-copy".into();
    rig.settle();
    assert!(rig.pasteboard.apply_pending(&mut mac));
    assert_eq!(mac.text, "genuine-later-guest-copy");
    assert!(rig.pasteboard.poll(&mut mac));
    assert_eq!(rig.h.pasteboard.as_ref().unwrap().try_changed(), None);
}

#[test]
fn observed_mac_copy_before_guest_reply_is_preserved() {
    observed_mac_copy_survives_guest_reply(false);
}

#[test]
fn observed_mac_copy_after_queued_guest_reply_is_preserved() {
    observed_mac_copy_survives_guest_reply(true);
}
