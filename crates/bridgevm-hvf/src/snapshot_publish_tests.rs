use super::*;
use crate::snapshot_pair::snapshot_pair_tests::Scratch;

fn pair(path: &Path, value: &[u8]) {
    fs::create_dir(path).unwrap();
    fs::write(path.join("disk.raw"), value).unwrap();
    fs::write(path.join("vars.fd"), value).unwrap();
}

fn assert_pair(path: &Path, value: &[u8]) {
    assert_eq!(fs::read(path.join("disk.raw")).unwrap(), value);
    assert_eq!(fs::read(path.join("vars.fd")).unwrap(), value);
}

#[test]
fn publishes_a_new_complete_directory() {
    let s = Scratch::new("publish-new");
    pair(&s.path("stage"), b"new");
    publish(&s.path("stage"), &s.path("dest")).unwrap();
    assert_pair(&s.path("dest"), b"new");
    assert!(!s.path("stage").exists());
}

#[test]
fn replaces_an_existing_complete_directory() {
    let s = Scratch::new("publish-existing");
    pair(&s.path("stage"), b"new");
    pair(&s.path("dest"), b"old");
    publish(&s.path("stage"), &s.path("dest")).unwrap();
    assert_pair(&s.path("dest"), b"new");
    assert!(!s.path("stage").exists());
}

#[test]
fn failed_exchange_never_deletes_the_old_snapshot() {
    let s = Scratch::new("publish-refused");
    pair(&s.path("stage"), b"new");
    pair(&s.path("dest"), b"old");
    let result = publish_with(&s.path("stage"), &s.path("dest"), |_, _| {
        Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "injected exchange refusal",
        ))
    });
    assert!(result.is_err());
    assert_pair(&s.path("dest"), b"old");
    assert_pair(&s.path("stage"), b"new");
}

#[test]
fn failure_after_exchange_retains_two_complete_versions() {
    let s = Scratch::new("publish-post-exchange");
    pair(&s.path("stage"), b"new");
    pair(&s.path("dest"), b"old");
    let result = publish_with(&s.path("stage"), &s.path("dest"), |left, right| {
        exchange(left, right)?;
        Err(io::Error::other("injected failure before directory sync"))
    });
    assert!(result.is_err());
    assert_pair(&s.path("dest"), b"new");
    assert_pair(&s.path("stage"), b"old");
}

#[test]
fn refuses_a_file_destination_without_mutating_it() {
    let s = Scratch::new("publish-file");
    pair(&s.path("stage"), b"new");
    s.write("dest", b"unrelated");
    assert!(publish(&s.path("stage"), &s.path("dest")).is_err());
    assert_eq!(fs::read(s.path("dest")).unwrap(), b"unrelated");
    assert_pair(&s.path("stage"), b"new");
}

#[test]
fn refuses_a_symlink_destination_without_mutating_its_target() {
    let s = Scratch::new("publish-symlink");
    pair(&s.path("stage"), b"new");
    pair(&s.path("target"), b"old");
    std::os::unix::fs::symlink(s.path("target"), s.path("dest")).unwrap();
    assert!(publish(&s.path("stage"), &s.path("dest")).is_err());
    assert_pair(&s.path("target"), b"old");
    assert_pair(&s.path("stage"), b"new");
}
