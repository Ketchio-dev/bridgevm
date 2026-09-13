use super::*;

#[test]
fn reservation_skips_existing_directory_file_and_symlinks_without_mutation() {
    use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

    let root = fresh_temp_dir("bridgevm-fixture-collisions");
    let external = fresh_temp_dir("bridgevm-fixture-external-sentinel");
    let existing_directory = root.join("existing-directory");
    fs::create_dir(&existing_directory).unwrap();
    let directory_sentinel = existing_directory.join("sentinel");
    fs::write(&directory_sentinel, "preserve directory contents").unwrap();
    let existing_file = root.join("existing-file");
    fs::write(&existing_file, "preserve file contents").unwrap();
    fs::set_permissions(&existing_file, fs::Permissions::from_mode(0o400)).unwrap();
    let file_before = fs::metadata(&existing_file).unwrap();
    let external_sentinel = external.join("sentinel");
    fs::write(&external_sentinel, "preserve symlink target").unwrap();
    let existing_link = root.join("existing-link");
    symlink(&external, &existing_link).unwrap();
    let missing_target = root.join("absent-target");
    let dangling_link = root.join("dangling-link");
    symlink(&missing_target, &dangling_link).unwrap();
    let fresh = root.join("fresh");
    let mut candidates = [
        existing_directory.clone(),
        existing_file.clone(),
        existing_link.clone(),
        dangling_link.clone(),
        fresh.clone(),
    ]
    .into_iter();

    let reserved = reserve_with(|| candidates.next().expect("candidate fixture"), 5).unwrap();
    assert_eq!(reserved, fresh);
    assert!(reserved.read_dir().unwrap().next().is_none());
    assert_eq!(
        fs::read_to_string(&directory_sentinel).unwrap(),
        "preserve directory contents"
    );
    assert_eq!(
        fs::read_to_string(&existing_file).unwrap(),
        "preserve file contents"
    );
    let file_after = fs::metadata(&existing_file).unwrap();
    assert_eq!(
        (file_after.dev(), file_after.ino(), file_after.mode()),
        (file_before.dev(), file_before.ino(), file_before.mode())
    );
    assert_eq!(fs::read_link(&existing_link).unwrap(), external);
    assert_eq!(
        fs::read_to_string(&external_sentinel).unwrap(),
        "preserve symlink target"
    );
    assert_eq!(fs::read_link(&dangling_link).unwrap(), missing_target);
    assert!(!missing_target.exists());
    fs::remove_dir_all(&root).unwrap();
    assert_eq!(
        fs::read_to_string(&external_sentinel).unwrap(),
        "preserve symlink target"
    );
    fs::remove_dir_all(external).unwrap();
}

#[test]
fn reservation_does_not_retry_errors_other_than_existing_entries() {
    let root = fresh_temp_dir("bridgevm-fixture-create-error");
    let missing_parent = root.join("missing-parent");
    let mut calls = 0;
    let error = reserve_with(
        || {
            calls += 1;
            missing_parent.join("child")
        },
        MAX_RESERVATION_ATTEMPTS,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::NotFound);
    assert_eq!(calls, 1);
    assert!(!missing_parent.exists());
    assert!(root.read_dir().unwrap().next().is_none());
    fs::remove_dir(root).unwrap();
}

#[test]
fn reservation_exhaustion_is_finite_and_preserves_the_collision() {
    let root = fresh_temp_dir("bridgevm-fixture-exhaustion");
    let occupied = root.join("occupied");
    fs::write(&occupied, "do not remove this collision").unwrap();
    let mut calls = 0;
    let error = reserve_with(
        || {
            calls += 1;
            occupied.clone()
        },
        3,
    )
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    assert_eq!(calls, 3);
    assert_eq!(
        fs::read_to_string(&occupied).unwrap(),
        "do not remove this collision"
    );
    let error = reserve_with(|| panic!("zero attempts must not ask for a path"), 0).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn atomic_reservation_produces_128_distinct_directories_across_eight_threads() {
    use std::collections::HashSet;
    use std::sync::{Arc, Barrier};
    use std::thread;

    let root = fresh_temp_dir("bridgevm-fixture-reservation-race");
    let barrier = Arc::new(Barrier::new(8));
    let sequence = Arc::new(AtomicU64::new(0));
    let workers: Vec<_> = (0..8)
        .map(|_| {
            let root = root.clone();
            let barrier = Arc::clone(&barrier);
            let sequence = Arc::clone(&sequence);
            thread::spawn(move || {
                barrier.wait();
                (0..16)
                    .map(|iteration| {
                        let mut first = true;
                        reserve_with(
                            || {
                                if first {
                                    first = false;
                                    root.join(format!("shared-{iteration}"))
                                } else {
                                    root.join(format!(
                                        "fallback-{}",
                                        sequence.fetch_add(1, Ordering::Relaxed)
                                    ))
                                }
                            },
                            MAX_RESERVATION_ATTEMPTS,
                        )
                        .unwrap()
                    })
                    .collect::<Vec<_>>()
            })
        })
        .collect();
    // Join every worker before assertions or removal, including error cases.
    let joined: Vec<_> = workers.into_iter().map(|worker| worker.join()).collect();
    let directories: Vec<_> = joined
        .into_iter()
        .flat_map(|result| result.unwrap())
        .collect();
    assert_eq!(directories.len(), 128);
    let unique: HashSet<_> = directories.iter().collect();
    assert_eq!(unique.len(), 128);
    assert!(directories
        .iter()
        .all(|path| path.is_dir() && path.starts_with(&root)));
    assert_eq!(root.read_dir().unwrap().count(), 128);
    fs::remove_dir_all(root).unwrap();
}
