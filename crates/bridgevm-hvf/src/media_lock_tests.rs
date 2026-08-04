use super::*;

fn temp_image(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bv-medialock-{}-{name}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let image = dir.join("disk.img");
    std::fs::write(&image, b"disk").unwrap();
    image
}

#[test]
fn a_lease_can_be_taken_on_a_free_image() {
    let image = temp_image("free");
    let lease = MediaLease::acquire(&image, "test").expect("first writer");
    assert_eq!(lease.image_path(), image);
}

#[test]
fn a_second_writer_is_refused_while_the_lease_is_held() {
    // The whole point: two VMMs writing one image corrupts it.
    let image = temp_image("contended");
    let _first = MediaLease::acquire(&image, "first writer").expect("first writer");
    let second = MediaLease::acquire(&image, "second writer");
    assert!(matches!(second, Err(MediaLockError::Held { .. })));
}

#[test]
fn the_refusal_names_the_process_that_holds_the_image() {
    // "file is locked" sends someone hunting; naming the holder does not.
    let image = temp_image("named");
    let _first = MediaLease::acquire(&image, "bridgevm-runner").expect("first writer");
    let error = MediaLease::acquire(&image, "second").unwrap_err();
    let message = error.to_string();
    assert!(message.contains("bridgevm-runner"), "{message}");
    assert!(message.contains("disk.img"), "{message}");
}

#[test]
fn releasing_the_lease_lets_the_next_writer_in() {
    let image = temp_image("released");
    {
        let _first = MediaLease::acquire(&image, "first").expect("first writer");
    }
    MediaLease::acquire(&image, "second").expect("the lease must be released on drop");
}

#[test]
fn the_refusal_is_immediate_rather_than_blocking() {
    // A blocking lock would hang the UI on something it can never fairly win.
    let image = temp_image("nonblocking");
    let _first = MediaLease::acquire(&image, "first").expect("first writer");
    let started = std::time::Instant::now();
    let _ = MediaLease::acquire(&image, "second");
    assert!(
        started.elapsed() < std::time::Duration::from_millis(500),
        "acquire must fail fast, took {:?}",
        started.elapsed()
    );
}

#[test]
fn different_images_do_not_contend() {
    let first_image = temp_image("independent-a");
    let second_image = temp_image("independent-b");
    let _a = MediaLease::acquire(&first_image, "a").expect("first image");
    let _b = MediaLease::acquire(&second_image, "b").expect("second image");
}

#[test]
fn the_lock_lives_beside_the_image_not_in_a_temp_dir() {
    // A temp-dir lock is cleared by a reboot, which would silently permit the
    // double-writer case it exists to prevent.
    let image = temp_image("sidecar");
    let lock = lock_path_for(&image);
    assert_eq!(lock.parent(), image.parent());
    assert!(lock
        .file_name()
        .unwrap()
        .to_string_lossy()
        .ends_with(".lock"));
}

#[test]
fn the_lock_path_is_derived_from_the_image_name() {
    let lock = lock_path_for(Path::new("/tmp/example/win11.qcow2"));
    assert_eq!(
        lock,
        PathBuf::from("/tmp/example/win11.qcow2.bridgevm-writer.lock")
    );
}

#[test]
fn two_images_with_the_same_name_in_different_directories_are_separate() {
    let lock_a = lock_path_for(Path::new("/tmp/a/disk.img"));
    let lock_b = lock_path_for(Path::new("/tmp/b/disk.img"));
    assert_ne!(lock_a, lock_b);
}

#[test]
fn a_lease_survives_being_moved() {
    // Ownership moves through the VM builder; the lock must not release when
    // the value changes hands.
    let image = temp_image("moved");
    let lease = MediaLease::acquire(&image, "first").expect("first writer");
    let moved = lease;
    assert!(MediaLease::acquire(&image, "second").is_err());
    drop(moved);
    MediaLease::acquire(&image, "third").expect("released after the move");
}

#[test]
fn an_unwritable_directory_reports_io_rather_than_claiming_the_lease() {
    let error = MediaLease::acquire(Path::new("/no-such-root/disk.img"), "test").unwrap_err();
    assert!(matches!(error, MediaLockError::Io { .. }), "{error:?}");
}
