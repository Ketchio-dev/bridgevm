//! A new session (a VM start or guest restart) sends nothing for files both
//! sides already hold, and still sends or fetches what changed while the VM
//! was off.

use super::*;

#[test]
fn a_new_session_sends_only_what_changed_while_the_vm_was_off() {
    assert_eq!(dotnet_time(1_794_096_312_123), "2026-11-08T00:05:12.1230000Z");
    let mut rig = Rig::new("new-session");
    let chunked = vec![5u8; SHARE_PUT_CHUNK_BYTES + 3];
    for (name, bytes) in [("a.txt", &b"alpha"[..]), ("same-size.txt", b"12345"), ("big.bin", &chunked)] {
        std::fs::write(rig.dir.join(name), bytes).unwrap();
    }
    rig.run(16);
    assert_eq!(rig.guest.puts.len(), 3, "{:?}", rig.guest.puts);

    // With the VM off a Mac file changes at its size, and the guest gained a file.
    let changed = rig.dir.join("same-size.txt");
    let later = std::fs::metadata(&changed).unwrap().modified().unwrap() + Duration::from_secs(2);
    std::fs::write(&changed, b"54321").unwrap();
    std::fs::File::options().write(true).open(&changed).unwrap().set_modified(later).unwrap();
    rig.guest.write("guest-only.txt", b"g");
    rig.h.share = Some(ShareState::new(ShareSync::new(512), rig.dir.clone(), ROOT.into(), Duration::from_millis(500)));
    let sent = rig.guest.puts.len();
    rig.run(16);
    assert_eq!(rig.guest.puts[sent..], ["same-size.txt"]);
    assert_eq!(rig.guest.content("same-size.txt"), Some(&b"54321"[..]));
    assert_eq!(rig.guest.gets, ["guest-only.txt"]);
    assert_eq!(rig.host("guest-only.txt").as_deref(), Some(&b"g"[..]));
    rig.assert_settled();

    // A file recorded this way syncs as before: a Mac edit is sent.
    std::fs::write(rig.dir.join("a.txt"), b"alpha 2").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"alpha 2"[..]));
}

#[test]
fn a_failed_first_listing_still_lets_the_mac_files_go() {
    let mut rig = Rig::new("first-listing-fails");
    rig.guest.ls_error = true;
    std::fs::write(rig.dir.join("a.txt"), b"alpha").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.puts, ["a.txt"]);
}

#[test]
fn a_mac_file_deleted_while_it_waits_for_the_first_listing_releases_its_guest_copy() {
    let mut rig = Rig::new("left-while-waiting");
    rig.guest.write("left.txt", b"guest copy");
    std::fs::write(rig.dir.join("left.txt"), b"mac copy, never sent").unwrap();
    // One tick: the host scan holds the file back, the first listing sees the guest copy held.
    rig.run(1);
    assert!(rig.guest.puts.is_empty() && rig.guest.gets.is_empty());
    std::fs::remove_file(rig.dir.join("left.txt")).unwrap();
    rig.run(8);
    assert_eq!(rig.guest.gets, ["left.txt"], "the guest copy stayed held");
    assert_eq!(rig.host("left.txt").as_deref(), Some(&b"guest copy"[..]));
    // And it syncs from then on: a guest edit reaches the Mac.
    rig.guest.write("left.txt", b"guest edit");
    rig.run(8);
    assert_eq!(rig.host("left.txt").as_deref(), Some(&b"guest edit"[..]));
}

#[test]
fn a_mac_file_deleted_and_restored_with_its_time_is_sent_again_not_adopted() {
    let mut rig = Rig::new("restored");
    let path = rig.dir.join("kept.txt");
    std::fs::write(&path, b"precious").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("kept.txt"), Some(&b"precious"[..]));
    let mtime = std::fs::metadata(&path).unwrap().modified().unwrap();
    // Moved out of the share: the guest copy is deleted.
    std::fs::rename(&path, rig.dir.with_extension("aside")).unwrap();
    for _ in 0..8 {
        rig.run(1);
        if rig.guest.content("kept.txt").is_none() {
            break;
        }
    }
    assert_eq!(rig.guest.content("kept.txt"), None);
    // Moved back with its time before the next listing.
    std::fs::rename(rig.dir.with_extension("aside"), &path).unwrap();
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(mtime).unwrap();
    rig.run(12);
    assert_eq!(rig.host("kept.txt").as_deref(), Some(&b"precious"[..]), "the restored Mac file was deleted");
    assert_eq!(rig.guest.content("kept.txt"), Some(&b"precious"[..]));
}

#[test]
fn only_the_first_listing_is_adopted_from_and_never_a_copy_deleted_since() {
    let time = "2026-11-08T00:05:12.1230000Z";
    let listed = || LsEntry { name: "x.txt".into(), size: 3, is_dir: false, mtime: time.into() };
    let session = || {
        let mut engine = ShareSync::new(1024);
        let host = HostFile { name: "x.txt".into(), size: 3, mtime_ms: 1_794_096_312_123 };
        assert_eq!(engine.on_host_scan_normalized([host]).len(), 1);
        // The waiting Mac file holds the listed copy: nothing to fetch.
        assert!(engine.on_guest_listing_normalized([listed()]).is_empty());
        engine
    };
    let mut first = session();
    assert!(first.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123));
    // Deleted in the guest by the host's DEL, then the Mac file comes back: send it, never adopt it.
    first.on_guest_deleted("x.txt");
    assert!(!first.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123));
    let mut later = session();
    assert!(later.on_guest_listing_normalized([listed()]).is_empty());
    assert!(!later.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123), "adopted from a second listing");
}

#[test]
fn a_name_the_host_tried_to_send_is_never_adopted_from_the_listing() {
    let mut engine = ShareSync::new(1024);
    let host = |size, mtime_ms| HostFile { name: "x.txt".into(), size, mtime_ms };
    assert_eq!(engine.on_host_scan_normalized([host(4, 1_794_096_400_000)]).len(), 1);
    let listed = LsEntry { name: "x.txt".into(), size: 3, is_dir: false, mtime: "2026-11-08T00:05:12.1230000Z".into() };
    assert!(engine.on_guest_listing_normalized([listed]).is_empty());
    // The Mac file differs: the host sends it, and the PUT fails after the guest truncated its copy.
    assert!(!engine.adopt_listed_copy("x.txt", b"abcd", 1_794_096_400_000));
    assert!(engine.on_host_file("x.txt".into(), b"abcd".to_vec(), 1_794_096_400_000).is_some());
    engine.on_put_rejected("x.txt");
    // Restored to the listed size and time (cp -p): the guest copy is not the listed one any more.
    assert_eq!(engine.on_host_scan_normalized([host(3, 1_794_096_312_123)]).len(), 1);
    assert!(!engine.adopt_listed_copy("x.txt", b"abc", 1_794_096_312_123), "adopted a copy the PUT may have cut short");
}

fn now_ms() -> u128 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis()
}

const HOUR_MS: u128 = 3_600_000;

/// The VM's UEFI vars beside the share folder (never inside it); its synced state goes next to it.
fn vars(rig: &Rig) -> PathBuf {
    let vars = rig.dir.with_extension("vars.fd");
    for suffix in ["", ".share-sync", ".share-sync-puts"] {
        let mut path = vars.as_os_str().to_owned();
        path.push(suffix);
        let _ = std::fs::remove_file(path);
    }
    vars
}

/// A new session (a VM start or guest restart), with the state at `vars` when given.
fn new_session(rig: &mut Rig, vars: Option<&Path>) {
    rig.h.share = None; // the old session ends first and leaves its state on disk
    let mut share = ShareState::new(ShareSync::new(512), rig.dir.clone(), ROOT.into(), Duration::from_millis(500));
    if let Some(vars) = vars {
        share.attach_sync_state(vars);
    }
    rig.h.share = Some(share);
}

/// A file Windows made at `guest_ms` (its clock), synced to the Mac with that time.
fn synced_from_guest(label: &str, guest_ms: u128, state: bool) -> (Rig, PathBuf) {
    let mut rig = Rig::new(label);
    let vars = vars(&rig);
    new_session(&mut rig, state.then_some(vars.as_path()));
    rig.guest.write("doc.bin", b"old version");
    rig.guest.stamps.insert("DOC.BIN".into(), guest_ms);
    rig.run(8);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"old version"[..]));
    (rig, vars)
}

#[test]
fn a_guest_edit_a_restart_cut_off_reaches_the_mac_instead_of_the_older_mac_copy_overwriting_it() {
    // Live (share-interrupt-20261109 s2): Windows replaced a synced file, the guest rebooted while
    // the new version was copied to the Mac, and the next session sent the older Mac copy over it.
    // No clock decides: the guest's runs hours behind or ahead (a time zone read as UTC).
    for skew in [-5 * HOUR_MS as i128, 5 * HOUR_MS as i128] {
        let guest_now = |offset: u128| (now_ms() as i128 + skew) as u128 + offset;
        let (mut rig, vars) = synced_from_guest("guest-edit", guest_now(0), true);
        settle(&mut rig);
        rig.guest.write("doc.bin", b"new version, longer");
        rig.guest.stamps.insert("DOC.BIN".into(), guest_now(60_000));
        new_session(&mut rig, Some(&vars));
        let sent = rig.guest.puts.len();
        rig.run(16);
        assert_eq!(rig.guest.content("doc.bin"), Some(&b"new version, longer"[..]), "skew {skew}: the older Mac copy overwrote the guest edit");
        assert_eq!(rig.guest.puts[sent..], [] as [String; 0]);
        assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"new version, longer"[..]));
        rig.assert_settled();
    }
}

#[test]
fn a_mac_edit_while_the_vm_was_off_is_sent_though_the_guest_clock_runs_ahead() {
    let (mut rig, vars) = synced_from_guest("mac-edit", now_ms() + 5 * HOUR_MS, true);
    std::fs::write(rig.dir.join("doc.bin"), b"edited on the Mac").unwrap();
    new_session(&mut rig, Some(&vars));
    rig.run(16);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"edited on the Mac"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"edited on the Mac"[..]));
    assert_eq!(rig.guest.gets.iter().filter(|name| *name == "doc.bin").count(), 1, "the older guest copy was fetched again");
}

/// v0 then v1 made in Windows and synced; then the guest copy is back at v0 (a crash undid v1).
fn reverted(label: &str) -> (Rig, PathBuf, u128) {
    let v0 = now_ms() - 2 * HOUR_MS;
    let (mut rig, vars) = synced_from_guest(label, v0, true);
    rig.guest.write("doc.bin", b"version one");
    rig.guest.stamps.insert("DOC.BIN".into(), v0 + 60_000);
    rig.run(8);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"version one"[..]));
    settle(&mut rig);
    rig.guest.write("doc.bin", b"old version");
    rig.guest.stamps.insert("DOC.BIN".into(), v0);
    (rig, vars, v0)
}

#[test]
fn a_guest_copy_back_at_the_previous_synced_version_gets_the_mac_copy_again() {
    let (mut rig, vars, _) = reverted("reverted");
    new_session(&mut rig, Some(&vars));
    let gets = rig.guest.gets.len();
    rig.run(16);
    assert_eq!(rig.guest.gets.len(), gets, "{:?}", rig.guest.gets);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"version one"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"version one"[..]));
}

#[test]
fn a_put_cut_short_leaves_a_guest_copy_that_is_never_fetched() {
    // The Mac copy goes back to the reverted guest and is cut short: the guest truncated, wrote
    // part of it and stamped it with its own clock; then the session ends (a probe kill).
    let (mut rig, vars, _) = reverted("cut-short");
    rig.guest.failing.insert("doc.bin".into());
    new_session(&mut rig, Some(&vars));
    rig.run(4);
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"versi"[..]), "the PUT did not cut the copy short");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() + 3 * HOUR_MS);
    rig.guest.failing.clear();
    new_session(&mut rig, Some(&vars));
    let gets = rig.guest.gets.len();
    rig.run(16);
    assert_eq!(rig.guest.gets.len(), gets, "the partial guest copy was fetched: {:?}", rig.guest.gets);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"version one"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"version one"[..]));
}

#[test]
fn a_missing_corrupt_foreign_or_unwritable_state_keeps_the_mac_winning_and_the_share_going() {
    for case in ["missing", "corrupt", "other-share", "unwritable"] {
        let (mut rig, vars) = synced_from_guest(&format!("state-{case}"), now_ms() - HOUR_MS, true);
        let mut versions = vars.as_os_str().to_owned();
        versions.push(".share-sync");
        rig.h.share = None;
        let vars = match case {
            "missing" => {
                std::fs::remove_file(&versions).unwrap();
                vars
            }
            "corrupt" => {
                std::fs::write(&versions, b"bridgevm-share-sync 1\nroot \nseq x\n").unwrap();
                vars
            }
            "other-share" => {
                let text = std::fs::read_to_string(&versions).unwrap();
                let mut other = String::new();
                base64_encode_into(b"/elsewhere", &mut other);
                let root = text.lines().nth(1).unwrap().to_string();
                std::fs::write(&versions, text.replace(&root, &format!("root {other}"))).unwrap();
                vars
            }
            _ => rig.dir.with_extension("no-such-dir").join("vars.fd"),
        };
        rig.guest.write("doc.bin", b"new version, longer");
        rig.guest.stamps.insert("DOC.BIN".into(), now_ms());
        new_session(&mut rig, Some(&vars));
        rig.run(16);
        // As without a state: the Mac copy wins, and the share keeps syncing.
        assert_eq!(rig.guest.content("doc.bin"), Some(&b"old version"[..]), "{case}");
        std::fs::write(rig.dir.join("after.txt"), b"still syncing").unwrap();
        rig.run(8);
        assert_eq!(rig.guest.content("after.txt"), Some(&b"still syncing"[..]), "{case}");
        if case == "unwritable" {
            let mut marks = vars.as_os_str().to_owned();
            marks.push(".share-sync-puts");
            assert!(!Path::new(&marks).exists());
        }
    }
}

/// Past SYNC_SETTLE: the last transfer's mark is gone (in memory and, at the session end, on disk).
fn settle(rig: &mut Rig) {
    rig.run(share_sync::share_synced::SYNC_SETTLE.as_secs() as usize + 4);
    assert!(!rig.h.share.as_ref().unwrap().engine.marked("doc.bin"), "the mark did not settle");
}

/// The guest copy cut short by a force-stop before it reached the guest's disk: part of it,
/// stamped with the guest's clock (hours ahead).
fn cut_short(rig: &mut Rig) {
    let whole = rig.guest.content("doc.bin").unwrap().to_vec();
    rig.guest.write("doc.bin", &whole[..whole.len() / 2]);
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() + 4 * HOUR_MS);
}

#[test]
fn a_put_the_guest_had_not_settled_when_the_session_ended_is_sent_again_not_fetched() {
    let mut rig = Rig::new("put-unsettled");
    let vars = vars(&rig);
    new_session(&mut rig, Some(&vars));
    std::fs::write(rig.dir.join("doc.bin"), b"the Mac's version").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"the Mac's version"[..]));
    // Confirmed, then a force-stop within SYNC_SETTLE: Windows had not written it all.
    cut_short(&mut rig);
    new_session(&mut rig, Some(&vars));
    rig.run(16);
    assert!(rig.guest.gets.is_empty(), "the cut-short guest copy was fetched: {:?}", rig.guest.gets);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"the Mac's version"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"the Mac's version"[..]));
}

#[test]
fn a_fetched_copy_the_guest_had_not_settled_when_the_session_ended_is_not_fetched_again() {
    let (mut rig, vars) = synced_from_guest("get-unsettled", now_ms() - HOUR_MS, true);
    cut_short(&mut rig);
    new_session(&mut rig, Some(&vars));
    let gets = rig.guest.gets.len();
    rig.run(16);
    assert_eq!(rig.guest.gets.len(), gets, "the cut-short guest copy was fetched: {:?}", rig.guest.gets);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"old version"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"old version"[..]));
}

#[test]
fn a_guest_edit_after_the_last_transfer_settled_is_still_fetched() {
    let (mut rig, vars) = synced_from_guest("settled", now_ms() - HOUR_MS, true);
    settle(&mut rig);
    rig.guest.write("doc.bin", b"edited after it settled");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() - HOUR_MS + 60_000);
    new_session(&mut rig, Some(&vars));
    rig.run(16);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"edited after it settled"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"edited after it settled"[..]));
}

#[test]
fn one_scan_pass_of_several_puts_writes_the_marks_once() {
    let mut rig = Rig::new("mark-batch");
    let vars = vars(&rig);
    new_session(&mut rig, Some(&vars));
    rig.run(4);
    for k in 0..5 {
        std::fs::write(rig.dir.join(format!("f{k}.txt")), b"burst").unwrap();
    }
    rig.run(16);
    assert_eq!(rig.guest.puts.len(), 5, "{:?}", rig.guest.puts);
    let mut marks = vars.as_os_str().to_owned();
    marks.push(".share-sync-puts");
    let text = std::fs::read_to_string(marks).unwrap();
    assert_eq!(text.lines().nth(2), Some("seq 1"), "one mark write per PUT: {text}");
}

#[test]
fn a_snapshot_restore_drops_the_synced_state_so_the_rolled_back_guest_copy_is_not_fetched() {
    // snapshot_pair_cli restore rolls the guest disk and the vars back while the Mac folder stays
    // current; a state left beside the vars would take the guest's older copy for an edit.
    use bridgevm_hvf::snapshot_pair::{create_snapshot, restore_snapshot};
    let mut rig = Rig::new("snapshot-restore");
    let vm = rig.dir.with_extension("vm");
    let _ = std::fs::remove_dir_all(&vm);
    std::fs::create_dir(&vm).unwrap();
    let (disk, vars, snap) = (vm.join("disk.raw"), vm.join("vars.fd"), vm.join("snap"));
    std::fs::write(&disk, b"guest disk").unwrap();
    std::fs::write(&vars, b"uefi vars").unwrap();
    create_snapshot(&disk, &vars, &snap, "vm", false, 1 << 20).unwrap();
    // Since the snapshot, two Windows versions synced to the Mac.
    new_session(&mut rig, Some(&vars));
    for (content, age) in [(&b"version two"[..], 2 * HOUR_MS), (b"version three, current", HOUR_MS)] {
        rig.guest.write("doc.bin", content);
        rig.guest.stamps.insert("DOC.BIN".into(), now_ms() - age);
        rig.run(8);
        assert_eq!(rig.host("doc.bin").as_deref(), Some(content));
        settle(&mut rig);
    }
    rig.h.share = None;
    restore_snapshot(&snap, &disk, &vars, false).unwrap();
    // The restored guest disk holds the copy from before: neither version the state lists.
    rig.guest.write("doc.bin", b"version one, in the snapshot");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() - 3 * HOUR_MS);
    new_session(&mut rig, Some(&vars));
    let gets = rig.guest.gets.len();
    rig.run(16);
    assert_eq!(rig.guest.gets[gets..], [] as [String; 0], "the rolled-back guest copy was fetched");
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"version three, current"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"version three, current"[..]));
    rig.h.share = None; // the session's state thread ends before its files go
    std::fs::remove_dir_all(&vm).unwrap();
}

#[test]
fn a_put_waits_until_the_state_thread_has_its_mark_on_disk() {
    let mut rig = Rig::new("mark-gate");
    let vars = vars(&rig);
    new_session(&mut rig, Some(&vars));
    rig.run(4);
    let marks = rig.h.share.as_ref().and_then(ShareState::sync_marks).unwrap();
    marks.hold.store(true, std::sync::atomic::Ordering::Release);
    std::fs::write(rig.dir.join("gated.txt"), b"after its mark").unwrap();
    rig.run(6);
    assert_eq!(rig.guest.puts, [] as [String; 0], "a PUT went out before its mark was on disk");
    assert!(rig.h.share.as_ref().unwrap().held_put.is_some());
    marks.hold.store(false, std::sync::atomic::Ordering::Release);
    rig.run(4);
    assert_eq!(rig.guest.content("gated.txt"), Some(&b"after its mark"[..]));
    let mut path = vars.as_os_str().to_owned();
    path.push(".share-sync-puts");
    let text = std::fs::read_to_string(path).unwrap();
    assert!(text.lines().any(|line| line.starts_with("mark ")), "{text}");
}

/// Makes a folder writable again when dropped, so a failing test leaves nothing behind.
struct WritableAgain(PathBuf);

impl Drop for WritableAgain {
    fn drop(&mut self) {
        use std::os::unix::fs::PermissionsExt as _;
        let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o755));
    }
}

#[test]
fn a_put_waits_while_a_state_that_is_off_cannot_be_removed_and_goes_once_it_is() {
    // The state folder turns unwritable: a mark fails (this PUT's, or first a copy's from
    // Windows), and so does removing the state. A PUT sent without its mark, cut short, would
    // leave a guest copy the stale state takes for a guest edit; it waits instead, while the
    // copies from Windows go on.
    use std::os::unix::fs::PermissionsExt as _;
    for windows_first in [false, true] {
        let mut rig = Rig::new(if windows_first { "state-stuck-get" } else { "state-stuck-put" });
        let vm = rig.dir.with_extension("vm");
        let _ = std::fs::remove_dir_all(&vm);
        std::fs::create_dir(&vm).unwrap();
        let vars = vm.join("vars.fd");
        let side = |suffix: &str| {
            let mut path = vars.as_os_str().to_owned();
            path.push(suffix);
            PathBuf::from(path)
        };
        new_session(&mut rig, Some(&vars));
        rig.run(4);
        std::fs::write(rig.dir.join("first.txt"), b"synced").unwrap();
        rig.run(8);
        assert_eq!(rig.guest.content("first.txt"), Some(&b"synced"[..]));
        assert!(side(".share-sync-puts").exists());
        let _writable = WritableAgain(vm.clone());
        std::fs::set_permissions(&vm, std::fs::Permissions::from_mode(0o555)).unwrap();
        if windows_first {
            rig.guest.write("from-windows.txt", b"still fetched");
            rig.run(6);
            assert!(rig.h.share.as_ref().unwrap().sync_state_stuck(), "the copy's mark did not fail");
        }
        std::fs::write(rig.dir.join("held.txt"), b"after the state is gone").unwrap();
        if !windows_first {
            rig.guest.write("from-windows.txt", b"still fetched");
        }
        rig.run(8);
        let puts = &rig.guest.puts;
        assert!(!puts.iter().any(|name| name.ends_with("held.txt")), "windows first {windows_first}: a PUT went out past a stale state: {puts:?}");
        assert!(rig.h.share.as_ref().unwrap().held_put.is_some());
        assert_eq!(rig.host("from-windows.txt").as_deref(), Some(&b"still fetched"[..]), "the copies from Windows stopped");
        std::fs::set_permissions(&vm, std::fs::Permissions::from_mode(0o755)).unwrap();
        rig.run(4);
        assert_eq!(rig.guest.content("held.txt"), Some(&b"after the state is gone"[..]));
        for suffix in [".share-sync", ".share-sync-puts"] {
            assert!(!side(suffix).exists(), "{suffix} outlived the state");
        }
        rig.h.share = None;
        std::fs::remove_dir_all(&vm).unwrap();
    }
}

#[test]
fn a_versions_write_in_flight_when_the_state_turns_off_leaves_no_file_behind() {
    use crate::agent_console::state::sync_file::SyncStateFile;
    use std::sync::atomic::Ordering;
    let rig = Rig::new("state-off-race");
    let vars = vars(&rig);
    let (mut file, _, _, _) = SyncStateFile::open(&vars, &rig.dir);
    let mut engine = ShareSync::new(512);
    let marks = file.marks();
    marks.hold.store(true, Ordering::Release);
    let now = Instant::now();
    engine.put_confirmed("doc.bin", 3, Some(1_794_096_312_123), now);
    assert!(file.flush(&mut engine, now), "a versions write queued (held in the state thread)");
    file.turn_off();
    assert!(file.invalidate().unwrap());
    // A large copy from Windows published now marks nothing (and goes on: mark_fetched).
    assert!(crate::agent_console::state::sync_file::mark(&marks, ["fetched.bin"]).is_err());
    marks.hold.store(false, Ordering::Release);
    file.join_writer();
    for suffix in [".share-sync", ".share-sync-puts"] {
        let mut path = vars.as_os_str().to_owned();
        path.push(suffix);
        assert!(!Path::new(&path).exists(), "{suffix} written after the state was removed");
    }
}

#[test]
fn a_mac_edit_made_while_a_preferred_guest_copy_is_fetched_wins_and_is_sent() {
    // Codex on #572: the first scan found the Mac copy at its last synced version, so the
    // guest's edit was preferred; the Mac file changed while that copy was fetched. Without a
    // check right before the publish, the guest copy replaced the Mac edit.
    let (mut rig, vars) = synced_from_guest("mac-edit-mid-fetch", now_ms() - 2 * HOUR_MS, true);
    settle(&mut rig);
    rig.guest.write("doc.bin", b"new version, longer");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() - HOUR_MS);
    let mac = rig.dir.join("doc.bin");
    rig.guest.on_get = Some(Box::new(move |name: &str| {
        if name == "doc.bin" {
            std::fs::write(&mac, b"edited on the Mac mid-fetch").unwrap();
        }
    }));
    let fetches = |rig: &Rig| rig.guest.gets.iter().filter(|name| *name == "doc.bin").count();
    let before = fetches(&rig);
    new_session(&mut rig, Some(&vars));
    rig.run(16);
    assert_eq!(fetches(&rig), before + 1, "the preferred copy was not fetched once: {:?}", rig.guest.gets);
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"edited on the Mac mid-fetch"[..]), "the guest copy replaced the Mac edit");
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"edited on the Mac mid-fetch"[..]), "the Mac edit was not sent");
}

#[test]
fn a_state_with_an_unusable_marks_file_is_removed_so_a_later_mark_is_not_outranked() {
    // Codex on #572: a valid versions file (seq 5) beside a corrupt marks file. Starting from
    // seq 0 while that versions file survived, the session's first mark (seq 1) was dropped at
    // the next start as older than it, and the cut-short guest copy of the PUT it guarded was
    // fetched over the complete Mac file.
    let (mut rig, vars) = synced_from_guest("unusable-marks", now_ms() - 2 * HOUR_MS, true);
    settle(&mut rig);
    rig.h.share = None;
    let side = |suffix: &str| {
        let mut path = vars.as_os_str().to_owned();
        path.push(suffix);
        PathBuf::from(path)
    };
    let (versions, marks) = (side(".share-sync"), side(".share-sync-puts"));
    let text = std::fs::read_to_string(&versions).unwrap();
    let seq = text.lines().find(|line| line.starts_with("seq ")).unwrap().to_string();
    std::fs::write(&versions, text.replace(&seq, "seq 5")).unwrap();
    std::fs::write(&marks, b"bridgevm-share-puts 1\nnot a state\n").unwrap();
    // The guest copy differs (an older one): with no usable state the Mac copy is sent.
    rig.guest.write("doc.bin", b"reverted, from before");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() - 3 * HOUR_MS);
    new_session(&mut rig, Some(&vars));
    let kept = versions.exists() || marks.exists();
    let mut sent = false;
    for _ in 0..16 {
        rig.run(1);
        if rig.guest.puts.iter().any(|name| name == "doc.bin") {
            sent = true;
            break;
        }
    }
    assert!(sent, "the Mac copy was not sent: {:?}", rig.guest.puts);
    // The probe killed right after the PUT went out: the state files as they are now ...
    let on_disk = [&versions, &marks].map(|path| std::fs::read(path).ok());
    rig.h.share = None;
    for (path, bytes) in [&versions, &marks].into_iter().zip(on_disk) {
        match bytes {
            Some(bytes) => std::fs::write(path, bytes).unwrap(),
            None => drop(std::fs::remove_file(path)),
        }
    }
    // ... and the PUT cut short in the guest's cache: a partial copy stamped with its time.
    rig.guest.write("doc.bin", b"old ver");
    rig.guest.stamps.insert("DOC.BIN".into(), now_ms() + 4 * HOUR_MS);
    new_session(&mut rig, Some(&vars));
    let gets = rig.guest.gets.len();
    rig.run(16);
    assert_eq!(rig.guest.gets[gets..], [] as [String; 0], "the cut-short guest copy was fetched");
    assert_eq!(rig.host("doc.bin").as_deref(), Some(&b"old version"[..]));
    assert_eq!(rig.guest.content("doc.bin"), Some(&b"old version"[..]));
    assert!(!kept, "a state not taken whole stayed on disk");
}

/// Clears a file's user-immutable flag when dropped, so a failing test leaves nothing behind.
struct Mutable(std::ffi::CString);

impl Mutable {
    fn set(path: &Path) -> Self {
        let path = std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).unwrap();
        // SAFETY: a valid C path; UF_IMMUTABLE is the owner's own flag.
        assert_eq!(unsafe { libc::chflags(path.as_ptr(), libc::UF_IMMUTABLE as _) }, 0);
        Self(path)
    }
}

impl Drop for Mutable {
    fn drop(&mut self) {
        // SAFETY: as in `set`.
        unsafe { libc::chflags(self.0.as_ptr(), 0) };
    }
}

#[test]
fn an_unusable_state_that_cannot_be_removed_holds_the_puts_until_it_is() {
    // The marks file is corrupt and the versions file cannot be removed (it is immutable), while
    // the folder still takes new files: kept on, the session would number its marks from 0 next
    // to that file. It goes off instead, and PUTs wait until the file is gone.
    let (mut rig, vars) = synced_from_guest("unusable-stuck", now_ms() - 2 * HOUR_MS, true);
    settle(&mut rig);
    rig.h.share = None;
    let side = |suffix: &str| {
        let mut path = vars.as_os_str().to_owned();
        path.push(suffix);
        PathBuf::from(path)
    };
    let (versions, marks) = (side(".share-sync"), side(".share-sync-puts"));
    std::fs::write(&marks, b"not a state\n").unwrap();
    let immutable = Mutable::set(&versions);
    std::fs::write(rig.dir.join("held.txt"), b"after the state is gone").unwrap();
    new_session(&mut rig, Some(&vars));
    rig.run(8);
    assert!(!rig.guest.puts.iter().any(|name| name == "held.txt"), "a PUT went out beside an unusable state");
    assert!(rig.h.share.as_ref().unwrap().sync_state_stuck());
    drop(immutable);
    rig.run(4);
    assert_eq!(rig.guest.content("held.txt"), Some(&b"after the state is gone"[..]));
    assert!(!versions.exists() && !marks.exists());
}

#[test]
fn a_session_that_ended_writes_no_more_marks() {
    // A large copy from Windows publishes on its own thread, with the session's marks: once the
    // session has ended, its late mark must not replace the next session's file.
    let mut rig = Rig::new("ended-session");
    let vars = vars(&rig);
    new_session(&mut rig, Some(&vars));
    rig.run(4);
    std::fs::write(rig.dir.join("a.txt"), b"sent").unwrap();
    rig.run(8);
    let marks = rig.h.share.as_ref().and_then(ShareState::sync_marks).unwrap();
    rig.h.share = None;
    let mut path = vars.as_os_str().to_owned();
    path.push(".share-sync-puts");
    let before = std::fs::read(&path).unwrap();
    assert!(crate::agent_console::state::sync_file::mark(&marks, ["late.bin"]).is_err());
    assert_eq!(std::fs::read(&path).unwrap(), before, "an ended session wrote its marks file");
}

#[test]
fn a_name_whose_put_is_held_is_not_fetched_while_other_windows_files_still_are() {
    // Codex on #572: the state folder turned unwritable mid-session, so this PUT is held while
    // the listings go on. For a name new this session nothing else kept its older guest copy
    // from being fetched over the Mac file the PUT waits to send.
    use std::os::unix::fs::PermissionsExt as _;
    let mut rig = Rig::new("held-put-pull");
    let vm = rig.dir.with_extension("vm");
    let _ = std::fs::remove_dir_all(&vm);
    std::fs::create_dir(&vm).unwrap();
    let vars = vm.join("vars.fd");
    new_session(&mut rig, Some(&vars));
    rig.run(4);
    // A state on disk, so that once the folder is unwritable it cannot be removed either.
    std::fs::write(rig.dir.join("first.txt"), b"synced").unwrap();
    rig.run(8);
    assert_eq!(rig.guest.content("first.txt"), Some(&b"synced"[..]));
    let _writable = WritableAgain(vm.clone());
    std::fs::set_permissions(&vm, std::fs::Permissions::from_mode(0o555)).unwrap();
    std::fs::write(rig.dir.join("conflict.txt"), b"the Mac's, newest").unwrap();
    // Its mark fails on the state thread, then removing the state fails: held, stuck.
    for _ in 0..10 {
        rig.run(1);
        if rig.h.share.as_ref().unwrap().sync_state_stuck() {
            break;
        }
    }
    let share = rig.h.share.as_ref().unwrap();
    assert!(share.sync_state_stuck() && share.held_put.is_some(), "the PUT is not held");
    rig.guest.write("conflict.txt", b"an older guest copy");
    rig.guest.stamps.insert("CONFLICT.TXT".into(), now_ms() - HOUR_MS);
    rig.guest.write("other.txt", b"from windows");
    let gets = rig.guest.gets.len();
    rig.run(8);
    let fetched = &rig.guest.gets[gets..];
    assert!(!fetched.iter().any(|name| name == "conflict.txt"), "fetched while its PUT was held: {fetched:?}");
    assert_eq!(rig.host("conflict.txt").as_deref(), Some(&b"the Mac's, newest"[..]));
    assert_eq!(rig.host("other.txt").as_deref(), Some(&b"from windows"[..]), "the other Windows file stopped");
    std::fs::set_permissions(&vm, std::fs::Permissions::from_mode(0o755)).unwrap();
    rig.run(6);
    assert_eq!(rig.guest.content("conflict.txt"), Some(&b"the Mac's, newest"[..]));
    // The exclusion went with the PUT: a later Windows edit of the name is fetched.
    rig.guest.write("conflict.txt", b"edited in windows afterwards");
    rig.guest.stamps.insert("CONFLICT.TXT".into(), now_ms() + 60_000);
    rig.run(8);
    assert_eq!(rig.host("conflict.txt").as_deref(), Some(&b"edited in windows afterwards"[..]));
    rig.h.share = None;
    std::fs::remove_dir_all(&vm).unwrap();
}
