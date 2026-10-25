//! Guest-to-host clipboard size refusal (decision of 2026-10-23): a guest text
//! whose decoded size would exceed 8 MiB never reaches the Mac pasteboard and
//! is never truncated. Texts up to 8 MiB and host-to-guest sync are unchanged.

use super::*;
use crate::agent_console::host_pasteboard::PasteboardBackend;

const LIMIT: usize = 8 * 1024 * 1024;

/// Answer the next CLIPGET with any payload: the modelled guest holds only
/// valid UTF-8 text.
fn poll_guest(rig: &mut ClipRig, b64: &str) {
    rig.enqueue();
    assert_eq!(rig.send(), "CLIPGET\n");
    rig.h
        .handle_service_reply(&format!("CLIP {b64}"), None, None, rig.now);
    assert!(rig.h.in_flight.is_none());
}

#[test]
fn guest_text_one_byte_over_8_mib_never_reaches_the_mac() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "g".repeat(LIMIT + 1);
    rig.settle();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "oversized guest text written to the Mac pasteboard"
    );
}

#[test]
fn guest_text_of_exactly_8_mib_syncs_untruncated() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "g".repeat(LIMIT);
    rig.settle();
    let lens: Vec<usize> = rig.mac_sets().iter().map(String::len).collect();
    assert_eq!(lens, vec![LIMIT]);
}

#[test]
fn invalid_utf8_growing_past_8_mib_is_refused_not_truncated() {
    // Within 8 MiB as bytes; U+FFFD (3 bytes) makes the text 8 MiB + 1.
    let mut bytes = vec![b'a'; LIMIT - 2];
    bytes.push(0xff);
    let mut rig = ClipRig::synced("old");
    poll_guest(&mut rig, &base64_encode(&bytes));
    assert!(
        rig.mac_sets().is_empty(),
        "lossy text over 8 MiB written to the Mac pasteboard"
    );
    // Exactly 8 MiB after replacement is still adopted, whole.
    bytes.truncate(LIMIT - 3);
    bytes.push(0xff);
    poll_guest(&mut rig, &base64_encode(&bytes));
    let sets = rig.mac_sets();
    assert_eq!(sets.len(), 1);
    assert_eq!(sets[0].len(), LIMIT);
    assert!(sets[0].ends_with('\u{fffd}'));
}

#[test]
fn after_a_refusal_later_copies_sync_both_ways() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "g".repeat(LIMIT + 1);
    rig.settle();
    assert!(rig.mac_sets().is_empty());
    rig.guest = "guest-new".into();
    rig.settle();
    assert_eq!(rig.mac_sets(), vec!["guest-new".to_owned()]);
    rig.copy_on_mac("mac-new");
    rig.settle();
    assert_eq!(rig.guest, "mac-new");
}

#[test]
fn after_a_refusal_the_guest_text_is_unknown_so_any_mac_copy_is_pushed() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "g".repeat(LIMIT + 1);
    rig.settle();
    assert!(rig.mac_sets().is_empty());
    // Equal to the pre-refusal baseline, but the guest no longer holds it.
    rig.copy_on_mac("old");
    rig.settle();
    assert_eq!(rig.guest, "old");
    assert!(rig.mac_sets().is_empty());
}

#[test]
fn refusal_keeps_a_pending_rebaseline_so_a_late_paste_is_not_published() {
    let mut rig = ClipRig::synced("old");
    rig.guest = "g".repeat(LIMIT + 1);
    // A legacy paste was written; its Set-Clipboard lands after this poll.
    rig.h.last_synced.rebaseline = true;
    rig.settle();
    rig.guest = "typed-secret".into();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "late paste text written to the Mac pasteboard"
    );
    assert_eq!(rig.h.last_synced.text.as_deref(), Some("typed-secret"));
}

#[test]
fn each_refused_guest_text_is_counted_once() {
    let mut rig = ClipRig::synced("old");
    let first = "g".repeat(LIMIT + 1);
    rig.guest = first.clone();
    rig.settle();
    // The unchanged guest clipboard is polled again: still one refusal.
    rig.settle();
    assert_eq!(rig.h.last_synced.refusals.count, 1);
    rig.guest = "h".repeat(LIMIT + 1);
    rig.settle();
    assert_eq!(rig.h.last_synced.refusals.count, 2);
    rig.guest = "small".into();
    rig.settle();
    rig.guest = first;
    rig.settle();
    assert_eq!(rig.h.last_synced.refusals.count, 3);
    assert_eq!(rig.mac_sets(), vec!["small".to_owned()]);
}

#[test]
fn oversized_payload_is_refused_by_its_base64_length_before_decoding() {
    let mut rig = ClipRig::synced("old");
    // Not base64: decoding would fail and leave nothing to refuse or count.
    poll_guest(&mut rig, &"!".repeat((LIMIT + 3) / 3 * 4));
    assert_eq!(rig.h.last_synced.refusals.count, 1);
    // A malformed payload within the limit still decodes to nothing.
    poll_guest(&mut rig, "!!!!");
    assert_eq!(rig.h.last_synced.refusals.count, 1);
    assert!(rig.mac_sets().is_empty());
}

#[test]
fn base64_length_gives_the_exact_decoded_size() {
    for len in [0, 1, 2, 3, 4, 5, LIMIT - 1, LIMIT, LIMIT + 1] {
        assert_eq!(base64_decoded_len(&base64_encode(&vec![0xa5; len])), len);
    }
}

#[test]
fn refusal_line_is_bounded_and_carries_no_clipboard_content() {
    assert_eq!(
        guest_clip_refusal_line(9_437_184, 2, 1234),
        "BVAGENT CLIPSYNC refused guest-text too-large bytes=9437184 limit=8388608 refusals=2 t=1234"
    );
    assert!(guest_clip_refusal_line(usize::MAX, u64::MAX, u128::MAX).len() <= 160);
    // BvAgentEvent.swift reads a CLIPSYNC line as a completed sync only when
    // it names a direction; a refusal must not show up as one in the app.
    let swift = include_str!(
        "../../../../../apps/macos/Sources/BridgeVMControl/HvfEngine/BvAgentEvent.swift"
    );
    for direction in [" host->guest ", " guest->host "] {
        assert!(swift.contains(&format!("line.contains(\"{direction}\")")));
        assert!(!guest_clip_refusal_line(1, 1, 1).contains(direction));
    }
}

/// Counts native writes; never reports host text.
#[derive(Default)]
struct NativeWrites {
    count: i64,
    lens: Vec<usize>,
}

impl PasteboardBackend for NativeWrites {
    fn change_count(&mut self) -> i64 {
        self.count
    }
    fn get_string(&mut self) -> Option<String> {
        None
    }
    fn set_string(&mut self, text: &str) -> bool {
        self.lens.push(text.len());
        self.count += 1;
        true
    }
}

#[test]
fn pasteboard_worker_never_writes_guest_text_over_8_mib() {
    let (pasteboard, mut worker) = HostPasteboard::test_pair();
    let mut mac = NativeWrites::default();
    assert!(worker.poll(&mut mac));
    pasteboard.set("w".repeat(LIMIT + 1));
    assert!(worker.apply_pending(&mut mac));
    pasteboard.set("w".repeat(LIMIT));
    assert!(worker.apply_pending(&mut mac));
    assert_eq!(
        mac.lens,
        vec![LIMIT],
        "the worker wrote guest text over 8 MiB"
    );
}
