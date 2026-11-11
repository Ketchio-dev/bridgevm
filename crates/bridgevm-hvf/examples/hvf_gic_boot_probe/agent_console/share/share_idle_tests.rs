//! An idle share is not planned again (share_idle.rs): with FSEvents quiet and the agent
//! answering LSSAME, a poll runs neither the host pass nor the guest pass, and an edit either
//! way still lands. Debug builds also run each skipped pass on a copy of the engine and check
//! that it plans and changes nothing (ShareState::check_skipped_host_pass).

use super::*;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

fn idle_rig(label: &str) -> (Rig, Arc<AtomicU64>) {
    let mut rig = Rig::new(label);
    let changes = Arc::new(AtomicU64::new(0));
    rig.h.share.as_mut().unwrap().host_watch = HostWatch::manual(changes.clone());
    rig.h.note_agent_ready("GUEST v3-share2 lssame", rig.now);
    (rig, changes)
}

/// Host and guest passes so far.
fn passes(rig: &Rig) -> (usize, usize) {
    let share = rig.h.share.as_ref().unwrap();
    (share.host_passes, share.guest_passes)
}

/// FSEvents reports a change; the share then settles and stays idle.
fn settle(rig: &mut Rig, changes: &AtomicU64) -> (usize, usize) {
    changes.fetch_add(1, Ordering::Release);
    rig.run(6);
    passes(rig)
}

#[test]
fn an_idle_share_is_not_planned_again_and_edits_either_way_still_land() {
    let (mut rig, changes) = idle_rig("idle");
    std::fs::write(rig.dir.join("mac.txt"), b"from the mac").unwrap();
    rig.guest.write("win.txt", b"from windows");
    rig.run(12);
    assert_eq!(rig.guest.content("mac.txt"), Some(&b"from the mac"[..]));
    assert_eq!(rig.host("win.txt").as_deref(), Some(&b"from windows"[..]));

    let idle = settle(&mut rig, &changes);
    let same = rig.guest.same_replies;
    rig.run(20);
    assert!(rig.guest.same_replies >= same + 15, "the folder was listed whole");
    assert_eq!(passes(&rig), idle, "an idle share was planned again");

    // A Windows edit: the agent lists the folder whole, and the host plans it.
    rig.guest.write("win.txt", b"windows edit");
    rig.run(4);
    assert_eq!(rig.host("win.txt").as_deref(), Some(&b"windows edit"[..]));
    // A Mac edit FSEvents reports: a new walk, planned.
    std::fs::write(rig.dir.join("mac.txt"), b"mac edit").unwrap();
    changes.fetch_add(1, Ordering::Release);
    rig.run(4);
    assert_eq!(rig.guest.content("mac.txt"), Some(&b"mac edit"[..]));

    let idle = settle(&mut rig, &changes);
    rig.run(20);
    assert_eq!(passes(&rig), idle, "the share was planned again after the edits");
    rig.assert_settled();
}

#[test]
fn a_failed_write_keeps_the_kept_listing_planned_until_it_lands() {
    // As before: an LSSAME plans the GET whose write failed, on every poll.
    use std::os::unix::fs::PermissionsExt;
    let mode = |rig: &Rig, mode| std::fs::set_permissions(&rig.dir, std::fs::Permissions::from_mode(mode)).unwrap();
    let (mut rig, changes) = idle_rig("idle-retry");
    mode(&rig, 0o555);
    rig.guest.write("a.txt", b"guest");
    rig.run(12);
    let (passes_then, gets) = (passes(&rig), rig.guest.gets.len());
    rig.run(10);
    assert!(passes(&rig).1 >= passes_then.1 + 5, "a retry was skipped as idle");
    assert!(rig.guest.gets.len() >= gets + 5);
    mode(&rig, 0o755);
    rig.run(6);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"guest"[..]));
    let idle = settle(&mut rig, &changes);
    rig.run(10);
    assert_eq!(passes(&rig), idle);
}

#[test]
fn a_new_agent_process_is_planned_from_its_whole_listing() {
    let (mut rig, changes) = idle_rig("idle-ready");
    rig.guest.write("a.txt", b"guest");
    rig.run(8);
    let idle = settle(&mut rig, &changes);
    rig.h.note_agent_ready("GUEST v3-share2 lssame", rig.now);
    rig.run(3);
    assert!(passes(&rig).1 > idle.1, "a new agent's listing was not planned");
    let idle = passes(&rig);
    rig.run(10);
    assert_eq!(passes(&rig), idle);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"guest"[..]));
}
