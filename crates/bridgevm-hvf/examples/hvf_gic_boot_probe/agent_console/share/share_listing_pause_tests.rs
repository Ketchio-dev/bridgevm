//! A guest folder past the listing limit (share-bigloss-20261111: 4200 files): while its
//! listing is refused for its size the host cannot tell a newer Windows edit, so nothing goes
//! over a guest copy; once a listing is applied the share goes on as before.

use super::adopt_tests::{new_session, vars};
use super::*;

/// Windows files enough to take the guest listing past its entry limit.
fn grow(guest: &mut NtfsGuest) {
    let bulk = (0..share_sync::MAX_LISTING_ENTRIES).map(|i| (format!("bulk\\f{i}"), Vec::new(), 0));
    guest.files.extend(bulk);
}

fn shrink(guest: &mut NtfsGuest) {
    guest.files.retain(|(name, _, _)| !name.starts_with("bulk\\"));
}

fn sorted(names: &[String]) -> Vec<String> {
    let mut names = names.to_vec();
    names.sort();
    names
}

#[test]
fn a_new_session_whose_first_listing_is_too_large_sends_nothing_over_a_windows_edit() {
    // Live (share-bigloss-20261111 boot 2): every Mac file was sent again and the edit was lost.
    let mut rig = Rig::new("too-large-new-session");
    let vars = vars(&rig);
    new_session(&mut rig, Some(&vars));
    std::fs::write(rig.dir.join("a.txt"), b"alpha").unwrap();
    std::fs::write(rig.dir.join("b.txt"), b"beta").unwrap();
    rig.run(12);
    assert_eq!(sorted(&rig.guest.puts), ["a.txt", "b.txt"]);
    // The guest folder grows past the limit and Windows edits a file; the VM starts again.
    grow(&mut rig.guest);
    rig.guest.write("a.txt", b"windows edit");
    new_session(&mut rig, Some(&vars));
    std::fs::write(rig.dir.join("new on mac.txt"), b"new").unwrap();
    let sent = rig.guest.puts.len();
    let _ = ShareState::take_pause_lines();
    rig.run(16);
    assert_eq!(rig.guest.puts[sent..], [] as [String; 0], "sent while the guest listing was refused");
    assert_eq!(rig.guest.content("a.txt"), Some(&b"windows edit"[..]));
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"alpha"[..]));
    assert!(rig.guest.gets.is_empty());
    assert_eq!(ShareState::take_pause_lines(), ["BVAGENT SHARE paused too-many-entries limit=4096"]);
}

#[test]
fn a_mac_edit_or_deletion_while_the_listing_is_too_large_waits_and_then_syncs() {
    let mut rig = Rig::new("too-large-mac-edit");
    std::fs::write(rig.dir.join("a.txt"), b"one").unwrap();
    std::fs::write(rig.dir.join("b.txt"), b"two").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content("b.txt"), Some(&b"two"[..]));
    let _ = ShareState::take_pause_lines();
    grow(&mut rig.guest);
    rig.run(3);
    assert_eq!(ShareState::take_pause_lines(), ["BVAGENT SHARE paused too-many-entries limit=4096"]);
    let sent = rig.guest.puts.len();
    std::fs::write(rig.dir.join("a.txt"), b"mac edit").unwrap();
    std::fs::remove_file(rig.dir.join("b.txt")).unwrap();
    rig.run(16);
    assert_eq!(rig.guest.puts[sent..], [] as [String; 0], "sent while the guest listing was refused");
    assert_eq!(rig.guest.content("a.txt"), Some(&b"one"[..]));
    assert_eq!(rig.guest.content("b.txt"), Some(&b"two"[..]), "deleted while the guest listing was refused");
    assert!(ShareState::take_pause_lines().is_empty(), "printed again while still paused");
    // The guest folder gets smaller: its listing is applied and the Mac changes go as before.
    shrink(&mut rig.guest);
    rig.run(16);
    assert_eq!(ShareState::take_pause_lines(), ["BVAGENT SHARE resumed"]);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"mac edit"[..]));
    assert_eq!(rig.guest.content("b.txt"), None);
    rig.assert_settled();
}

#[test]
fn an_applied_listing_after_a_too_large_one_adopts_the_copies_both_sides_hold() {
    let mut rig = Rig::new("too-large-then-adopt");
    std::fs::write(rig.dir.join("a.txt"), b"alpha").unwrap();
    std::fs::write(rig.dir.join("b.txt"), b"beta").unwrap();
    rig.run(12);
    grow(&mut rig.guest);
    new_session(&mut rig, None);
    let sent = rig.guest.puts.len();
    rig.run(8);
    assert_eq!(rig.guest.puts[sent..], [] as [String; 0], "sent while the guest listing was refused");
    shrink(&mut rig.guest);
    rig.run(16);
    assert_eq!(rig.guest.puts[sent..], [] as [String; 0], "copies of the same size and time were sent again");
    rig.assert_settled();
    // Recorded by that listing: a Mac edit is sent.
    std::fs::write(rig.dir.join("a.txt"), b"alpha 2").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"alpha 2"[..]));
}

#[test]
fn a_fresh_guest_without_the_folder_gets_it_and_then_every_mac_file() {
    let mut rig = Rig::new("fresh-guest");
    rig.guest.root_missing = true;
    std::fs::write(rig.dir.join("a.txt"), b"alpha").unwrap();
    std::fs::write(rig.dir.join("b.txt"), b"beta").unwrap();
    rig.run(12);
    assert!(!rig.guest.root_missing, "the guest folder was not created");
    assert_eq!(sorted(&rig.guest.puts), ["a.txt", "b.txt"]);
    rig.assert_settled();
}

#[test]
fn a_listing_too_large_in_bytes_pauses_too() {
    let mut rig = Rig::synced("too-large-bytes", "a.txt", b"one");
    let _ = ShareState::take_pause_lines();
    // Fewer entries than the limit, but names long enough to take the listing past its bytes.
    let folder = "d".repeat(150);
    let name = "n".repeat(150);
    let count = share_sync::MAX_LISTING_ENTRIES - 100;
    rig.guest.files.extend((0..count).map(|i| (format!("{folder}\\{name}{i}"), Vec::new(), 0)));
    rig.run(3);
    assert_eq!(ShareState::take_pause_lines(), ["BVAGENT SHARE paused too-many-bytes limit=1048576"]);
    std::fs::write(rig.dir.join("a.txt"), b"mac edit").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"one"[..]));
}
