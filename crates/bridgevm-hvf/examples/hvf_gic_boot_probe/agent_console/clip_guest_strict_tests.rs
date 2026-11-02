//! Invalid replies are not clipboard snapshots and cannot consume paste privacy.

use super::*;

#[test]
fn malformed_base64_does_not_publish_a_later_legacy_paste() {
    let mut rig = ClipRig::synced("host-before-paste");
    rig.h.last_synced.rebaseline = true;
    poll_guest(&mut rig, "!!!!");
    let privacy_still_pending = rig.h.last_synced.rebaseline;
    rig.guest = "fixture-private-paste".into();
    rig.settle();
    assert!(
        rig.mac_sets().is_empty(),
        "malformed reply exposed the later app paste"
    );
    assert!(
        privacy_still_pending,
        "invalid text consumed the paste rebaseline"
    );
    assert_eq!(
        rig.h.last_synced.text.as_deref(),
        Some("fixture-private-paste")
    );
    rig.guest = "genuine-later-guest-copy".into();
    rig.settle();
    assert_eq!(rig.mac_sets(), ["genuine-later-guest-copy"]);
}

#[test]
fn invalid_utf8_is_not_replaced_and_later_copies_recover_both_ways() {
    let mut rig = ClipRig::synced("host-before-invalid-text");
    let invalid = base64_encode(&[0xff]);
    poll_guest(&mut rig, &invalid);
    poll_guest(&mut rig, &invalid);
    assert!(
        rig.mac_sets().is_empty(),
        "invalid UTF8 was replaced onto the Mac"
    );
    assert_eq!(rig.h.last_synced.refusals.count, 1);
    assert!(rig.h.last_synced.text.is_none());
    rig.guest = "later-valid-guest-copy".into();
    rig.settle();
    assert_eq!(rig.mac_sets(), ["later-valid-guest-copy"]);
    rig.copy_on_mac("later-valid-host-copy");
    rig.settle();
    assert_eq!(rig.guest, "later-valid-host-copy");
    assert!(rig.mac_sets().is_empty());
}

#[test]
fn valid_empty_reply_keeps_the_existing_ignore_and_rebaseline_policy() {
    let mut rig = ClipRig::synced("host-before-empty");
    rig.h.last_synced.rebaseline = true;
    poll_guest(&mut rig, "");
    assert!(rig.mac_sets().is_empty());
    assert_eq!(rig.h.last_synced.refusals.count, 0);
    assert_eq!(rig.h.last_synced.text.as_deref(), Some("host-before-empty"));
    assert!(!rig.h.last_synced.rebaseline);
}

#[test]
fn invalid_decode_records_are_bounded_fixed_reasons_and_not_completed_syncs() {
    for (error, reason) in [
        (GuestClipError::InvalidBase64, "invalid-base64"),
        (GuestClipError::InvalidUtf8, "invalid-utf8"),
    ] {
        assert_eq!(
            guest_clip_error_line(error, 2, 1234),
            format!("BVAGENT CLIPSYNC refused guest-text {reason} refusals=2 t=1234")
        );
        let line = guest_clip_error_line(error, u64::MAX, u128::MAX);
        assert!(line.len() <= 160);
        assert!(!line.contains(" guest->host ") && !line.contains(" host->guest "));
    }
}
