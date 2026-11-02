//! Guest replies cannot overwrite or delete a replacement share root's files.

use super::*;

struct ReplacedRoot {
    holder: PathBuf,
    original: PathBuf,
}

impl ReplacedRoot {
    fn new(rig: &Rig, label: &str) -> Self {
        for nonce in 0..128 {
            let holder = std::env::temp_dir().join(format!(
                "bvagent-guest-destination-{label}-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::create_dir(&holder) {
                Ok(()) => {
                    let original = holder.join("original");
                    let owned = Self { holder, original };
                    std::fs::rename(&rig.dir, &owned.original).unwrap();
                    std::fs::create_dir(&rig.dir).unwrap();
                    std::fs::write(rig.dir.join("a.txt"), b"unrelated sentinel").unwrap();
                    return owned;
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned root holder: {error}"),
            }
        }
        panic!("no fresh root holder available");
    }

    fn restore(&self, rig: &Rig) {
        std::fs::remove_file(rig.dir.join("a.txt")).unwrap();
        std::fs::remove_dir(&rig.dir).unwrap();
        std::fs::rename(&self.original, &rig.dir).unwrap();
    }
}

impl Drop for ReplacedRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.holder);
    }
}

fn dispatch(rig: &mut Rig) -> String {
    let mut sent = Vec::new();
    rig.h.service_pump_with_sender(rig.now, &mut |bytes| {
        sent.push(String::from_utf8(bytes.to_vec()).unwrap())
    });
    assert_eq!(sent.len(), 1);
    sent.pop().unwrap()
}

fn deliver(rig: &mut Rig, request: &str) {
    let mut packet = rig.guest.reply(request).join("\n");
    packet.push('\n');
    rig.h.inbound_scratch.extend_from_slice(packet.as_bytes());
    rig.h.advance(rig.now, &mut |_| {
        panic!("reply unexpectedly emitted another wire request")
    });
}

#[test]
fn a_pending_get_refuses_a_replacement_root_without_adopting_its_version() {
    let mut rig = Rig::synced("guest-root-get", "a.txt", b"original A");
    let (_, bytes, stamp) = rig
        .guest
        .files
        .iter()
        .find(|(name, _, _)| name == "a.txt")
        .unwrap();
    let baseline = LsEntry {
        name: "a.txt".into(),
        size: bytes.len() as u64,
        is_dir: false,
        mtime: format!("m{stamp}"),
    };
    rig.guest.write("a.txt", b"guest changed A");
    rig.run(1);
    let request = dispatch(&mut rig);
    assert_eq!(
        request,
        format!(
            "GET {}\n",
            base64_encode(format!("{ROOT}\\a.txt").as_bytes())
        )
    );
    let replacement = ReplacedRoot::new(&rig, "get");
    deliver(&mut rig, &request);
    assert_eq!(
        rig.host("a.txt").as_deref(),
        Some(&b"unrelated sentinel"[..])
    );
    assert_eq!(
        std::fs::read(replacement.original.join("a.txt")).unwrap(),
        b"original A"
    );
    assert!(
        rig.h.in_flight.is_none(),
        "valid GETEND did not release its wire slot"
    );
    assert!(
        rig.h
            .share
            .as_mut()
            .unwrap()
            .engine
            .on_guest_listing_normalized([baseline])
            .is_empty(),
        "a refused root advanced the successful guest baseline"
    );
    replacement.restore(&rig);
    rig.run(12);
    assert_eq!(rig.host("a.txt").as_deref(), Some(&b"guest changed A"[..]));
    rig.assert_settled();
}

#[test]
fn a_guest_delete_refuses_a_replacement_root_and_retries_after_restoration() {
    let mut rig = Rig::synced("guest-root-delete", "a.txt", b"original A");
    rig.guest.files.clear();
    rig.now += Duration::from_secs(1);
    rig.h.service_enqueue(rig.now);
    let request = dispatch(&mut rig);
    assert!(request.starts_with("LSR "));
    let replacement = ReplacedRoot::new(&rig, "delete");
    deliver(&mut rig, &request);
    assert_eq!(
        rig.host("a.txt").as_deref(),
        Some(&b"unrelated sentinel"[..])
    );
    assert_eq!(
        std::fs::read(replacement.original.join("a.txt")).unwrap(),
        b"original A"
    );
    assert!(rig.h.in_flight.is_none());
    let actions = rig
        .h
        .share
        .as_mut()
        .unwrap()
        .engine
        .on_guest_listing_normalized([]);
    assert!(
        matches!(actions.as_slice(), [SyncAction::DeleteHost { name }] if name == "a.txt"),
        "a refused root consumed the host deletion baseline"
    );
    replacement.restore(&rig);
    rig.run(12);
    assert!(rig.host("a.txt").is_none());
    assert!(rig.guest.content("a.txt").is_none());
    rig.assert_settled();
}
