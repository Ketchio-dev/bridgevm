//! Root replacement must not turn a temporary empty directory into guest deletions.

use super::*;
use std::os::unix::fs::symlink;

struct OwnedHolder(PathBuf);

impl OwnedHolder {
    fn new(label: &str) -> Self {
        for nonce in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "bvagent-root-identity-{label}-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned holder: {error}"),
            }
        }
        panic!("no fresh holder available");
    }

    fn replace_root(&self, rig: &Rig) -> PathBuf {
        let original = self.0.join("original");
        std::fs::rename(&rig.dir, &original).unwrap();
        std::fs::create_dir(&rig.dir).unwrap();
        original
    }
}

impl Drop for OwnedHolder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn restore_root(rig: &Rig, original: &Path) {
    std::fs::remove_dir(&rig.dir).unwrap();
    std::fs::rename(original, &rig.dir).unwrap();
}

fn pump(rig: &mut Rig) -> Vec<String> {
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |bytes| {
        sent.push(String::from_utf8(bytes.to_vec()).unwrap())
    });
    sent
}

#[test]
fn a_replacement_empty_root_keeps_guest_data_and_recovers_on_the_original() {
    let mut rig = Rig::synced("root-replaced", "a.txt", b"original A");
    let holder = OwnedHolder::new("replacement");
    let original = holder.replace_root(&rig);
    rig.run(12);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"original A"[..]));
    assert_eq!(
        std::fs::read(original.join("a.txt")).unwrap(),
        b"original A"
    );
    assert!(rig.host_names().is_empty());
    restore_root(&rig, &original);
    rig.guest.write("a.txt", b"guest edit after restoration");
    rig.run(12);
    assert_eq!(
        rig.host("a.txt").as_deref(),
        Some(&b"guest edit after restoration"[..])
    );
    rig.assert_settled();
}

#[test]
fn a_queued_guest_delete_waits_for_the_original_root_to_return() {
    let mut rig = Rig::synced("root-queued-delete", "a.txt", b"A");
    std::fs::remove_file(rig.dir.join("a.txt")).unwrap();
    assert!(rig.h.enqueue_one_share_host_change());
    assert!(matches!(rig.h.queue.front(), Some(ServiceReq::ShareDel {
        name, direction: ShareDelDirection::HostToGuest
    }) if name == "a.txt"));
    let holder = OwnedHolder::new("queued-delete");
    let original = holder.replace_root(&rig);
    assert!(
        pump(&mut rig).is_empty(),
        "queued DEL escaped through a replaced root"
    );
    assert!(rig.h.in_flight.is_none());
    assert_eq!(rig.guest.content("a.txt"), Some(&b"A"[..]));
    restore_root(&rig, &original);
    let sent = pump(&mut rig);
    assert_eq!(
        sent,
        [format!(
            "DEL {}\n",
            base64_encode(format!("{ROOT}\\a.txt").as_bytes())
        )]
    );
    for reply in rig.guest.reply(&sent[0]) {
        rig.h.handle_service_reply(&reply, None, None, rig.now);
    }
    assert!(rig.guest.content("a.txt").is_none());
    assert!(rig.h.in_flight.is_none());
}

#[test]
fn deleting_the_last_file_in_the_same_root_still_deletes_the_guest_copy() {
    let mut rig = Rig::synced("root-real-empty", "a.txt", b"A");
    let identity = std::fs::metadata(&rig.dir).unwrap();
    std::fs::remove_file(rig.dir.join("a.txt")).unwrap();
    rig.run(12);
    assert!(rig.guest.content("a.txt").is_none());
    assert!(rig.host_names().is_empty());
    let current = std::fs::metadata(&rig.dir).unwrap();
    assert_eq!(
        (current.dev(), current.ino()),
        (identity.dev(), identity.ino())
    );
    rig.assert_settled();
}

#[test]
fn a_selected_symlink_root_accepts_its_original_target_and_refuses_a_swapped_target() {
    let mut rig = Rig::new("root-link");
    let holder = OwnedHolder::new("link");
    let original = holder.0.join("original");
    std::fs::rename(&rig.dir, &original).unwrap();
    symlink(&original, &rig.dir).unwrap();
    rig.h.share = Some(ShareState::new(
        ShareSync::new(512),
        rig.dir.clone(),
        ROOT.into(),
        Duration::from_millis(500),
    ));
    std::fs::write(rig.dir.join("a.txt"), b"linked A").unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"linked A"[..]));
    let empty = holder.0.join("empty");
    std::fs::create_dir(&empty).unwrap();
    std::fs::remove_file(&rig.dir).unwrap();
    symlink(&empty, &rig.dir).unwrap();
    rig.run(12);
    assert_eq!(rig.guest.content("a.txt"), Some(&b"linked A"[..]));
    assert_eq!(std::fs::read(original.join("a.txt")).unwrap(), b"linked A");
    std::fs::remove_file(&rig.dir).unwrap();
    symlink(&original, &rig.dir).unwrap();
    rig.guest
        .write("a.txt", b"guest edit through restored link");
    rig.run(12);
    assert_eq!(
        std::fs::read(original.join("a.txt")).unwrap(),
        b"guest edit through restored link"
    );
    rig.assert_settled();
}
