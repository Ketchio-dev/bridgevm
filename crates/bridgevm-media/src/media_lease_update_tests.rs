use super::*;
use crate::media_lease::tests::Scratch;

#[test]
fn duplicate_descriptions_do_not_delay_owner_unlock() {
    let s = Scratch::new("lease-duplicate-lifetime");
    let disk = s.write("disk", b"disk");
    let lease = MediaLease::acquire([disk.as_path()]).unwrap();
    let copies: Vec<_> = lease
        .files
        .values()
        .map(|f| f.try_clone().unwrap())
        .collect();
    drop(lease);
    let next = MediaLease::acquire([disk.as_path()]).unwrap();
    drop(copies);
    assert!(MediaLease::acquire([disk.as_path()]).is_err());
    drop(next);
    MediaLease::acquire([disk.as_path()]).unwrap();
}

#[test]
fn extension_reuses_owned_keys_and_adds_hardlink_protection() {
    let s = Scratch::new("lease-extend");
    let first = s.write("first", b"first");
    let second = s.write("second", b"second");
    let alias = s.path("second-alias");
    fs::hard_link(&second, &alias).unwrap();
    let mut lease = MediaLease::acquire([first.as_path()]).unwrap();
    lease.extend([first.as_path(), second.as_path()]).unwrap();
    let count = lease.files.len();
    lease.extend([first.as_path(), second.as_path()]).unwrap();
    assert_eq!(lease.files.len(), count);
    for path in [&first, &second, &alias] {
        assert_eq!(
            MediaLease::acquire([path.as_path()]).unwrap_err().kind(),
            io::ErrorKind::WouldBlock
        );
    }
}

#[test]
fn partial_update_failure_preserves_prior_locks_and_releases_additions() {
    for extend in [false, true] {
        let s = Scratch::new("lease-update-refused");
        let old = s.write("old", b"old");
        let first = s.write("first", b"first");
        let second = s.write("second", b"second");
        let mut lease = MediaLease::acquire([old.as_path()]).unwrap();
        let prior: Vec<_> = lease.files.keys().cloned().collect();
        let mut requested = BTreeSet::new();
        for path in [&first, &second] {
            requested.extend(resource_keys(path).unwrap());
        }
        // Lock only the greatest new key, forcing at least one successful
        // acquisition before update reaches the conflict in sorted order.
        assert!(requested.len() > 1);
        let blocked = requested.last().unwrap().clone();
        let uid = os::uid();
        let root = os::private_root(uid).unwrap();
        let blocker = MediaLease {
            files: BTreeMap::from([(blocked.clone(), lock_key(&root, &blocked, uid).unwrap())]),
        };
        let result = if extend {
            lease.extend([first.as_path(), second.as_path()])
        } else {
            lease.replace([first.as_path(), second.as_path()])
        };
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::WouldBlock);
        assert_eq!(lease.files.keys().cloned().collect::<Vec<_>>(), prior);
        assert!(MediaLease::acquire([old.as_path()]).is_err());
        // Check every earlier key was released while the final blocker remains.
        for key in requested.iter().filter(|key| **key != blocked) {
            let probe = lock_key(&root, key, uid).unwrap();
            os::lock(&probe, libc::LOCK_UN).unwrap();
        }
        drop(blocker);
        MediaLease::acquire([first.as_path(), second.as_path()]).unwrap();
    }
}

#[test]
fn replacement_unlocks_removed_keys_even_with_duplicate_descriptors() {
    let s = Scratch::new("lease-replace-duplicate");
    let old = s.write("old", b"old");
    let new = s.write("new", b"new");
    let mut lease = MediaLease::acquire([old.as_path()]).unwrap();
    let inherited: Vec<_> = lease
        .files
        .values()
        .map(|file| file.try_clone().unwrap())
        .collect();
    lease.replace([new.as_path()]).unwrap();
    let old_owner = MediaLease::acquire([old.as_path()]).unwrap();
    assert!(MediaLease::acquire([new.as_path()]).is_err());
    drop(inherited);
    assert!(MediaLease::acquire([old.as_path()]).is_err());
    drop(old_owner);
    lease.replace([]).unwrap();
    assert!(lease.files.is_empty());
    MediaLease::acquire([new.as_path()]).unwrap();
}

#[test]
fn successful_rebinding_keeps_a_bounded_set_of_descriptors() {
    let s = Scratch::new("lease-replace-bounded");
    let logical = s.write("logical", b"logical");
    let current = s.write("current", b"initial");
    let staging = s.path("staging");
    let mut lease = MediaLease::acquire([logical.as_path(), current.as_path()]).unwrap();
    let count = lease.files.len();
    for generation in 0..16 {
        fs::write(&staging, generation.to_string()).unwrap();
        lease.extend([staging.as_path()]).unwrap();
        fs::rename(&staging, &current).unwrap();
        lease
            .replace([logical.as_path(), current.as_path()])
            .unwrap();
        assert_eq!(lease.files.len(), count);
        for path in [&logical, &current] {
            assert!(MediaLease::acquire([path.as_path()]).is_err());
        }
    }
}
