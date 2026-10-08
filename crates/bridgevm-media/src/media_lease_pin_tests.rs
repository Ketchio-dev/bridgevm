use super::*;

#[test]
fn pin_survives_rebinding_and_releases_original_inode_on_drop() {
    let s = Scratch::new("lease-pin");
    let input = s.write("input", b"input");
    let alias = s.path("old-alias");
    fs::hard_link(&input, &alias).unwrap();
    let next = s.write("next", b"next");
    let mut owner = MediaLease::acquire([input.as_path()]).unwrap();
    owner.pin([input.as_path()]).unwrap();
    let pin_count = owner.pinned.len();
    owner.pin([input.as_path()]).unwrap();
    assert_eq!(owner.pinned.len(), pin_count);
    owner.extend([next.as_path()]).unwrap();
    fs::rename(&next, &input).unwrap();
    owner.replace([input.as_path()]).unwrap();
    for path in [&input, &alias] {
        assert_eq!(
            MediaLease::acquire([path.as_path()]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
    owner.replace([]).unwrap();
    assert_eq!(owner.files.len(), pin_count);
    assert_eq!(
        MediaLease::acquire([alias.as_path()]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    // The pinned pathname remains owned even if it later names a new inode.
    assert_eq!(
        MediaLease::acquire([input.as_path()]).unwrap_err().kind(),
        io::ErrorKind::WouldBlock
    );
    drop(owner);
    MediaLease::acquire([input.as_path(), alias.as_path()]).unwrap();
}

#[test]
fn partial_pin_failure_changes_neither_pins_nor_existing_ownership() {
    let s = Scratch::new("lease-pin-error");
    let old = s.write("old", b"old");
    let first = s.write("first", b"first");
    let second = s.write("second", b"second");
    let mut owner = MediaLease::acquire([old.as_path()]).unwrap();
    owner.pin([old.as_path()]).unwrap();
    let before: Vec<_> = owner.files.keys().cloned().collect();
    let pins = owner.pinned.clone();
    let requested: BTreeSet<_> = [&first, &second]
        .into_iter()
        .flat_map(|p| resource_keys(p).unwrap())
        .collect();
    let blocked = requested.last().unwrap().clone();
    let uid = os::uid();
    let root = os::private_root(uid).unwrap();
    let blocker = MediaLease {
        files: BTreeMap::from([(blocked.clone(), lock_key(&root, &blocked, uid).unwrap())]),
        pinned: BTreeSet::new(),
    };
    assert_eq!(
        owner
            .pin([first.as_path(), second.as_path()])
            .unwrap_err()
            .kind(),
        io::ErrorKind::WouldBlock
    );
    assert_eq!(owner.pinned, pins);
    assert_eq!(owner.files.keys().cloned().collect::<Vec<_>>(), before);
    for key in requested.iter().filter(|key| **key != blocked) {
        let probe = lock_key(&root, key, uid).unwrap();
        os::lock(&probe, libc::LOCK_UN).unwrap();
    }
    drop(blocker);
    MediaLease::acquire([first.as_path(), second.as_path()]).unwrap();
}
