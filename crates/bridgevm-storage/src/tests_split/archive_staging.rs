//! Reservation collisions preserve foreign entries and the first export's data.

use super::*;
use std::os::unix::fs::{symlink, MetadataExt, PermissionsExt};

#[test]
fn colliding_staging_candidates_keep_distinct_owners_and_cleanup() {
    let fixture = create().unwrap();
    let first_path = fixture.path.join("same-timestamp");
    let second_path = fixture.path.join("next-candidate");
    let first = reserve([first_path.clone()]).unwrap();
    fs::write(first.path.join("winner-data"), b"first export").unwrap();
    let first_identity = fs::metadata(&first.path).unwrap();
    let second = reserve([first_path.clone(), second_path.clone()]).unwrap();
    assert_eq!(second.path, second_path);
    assert_eq!(
        fs::metadata(&second.path).unwrap().permissions().mode() & 0o777,
        0o700
    );
    drop(second);
    assert!(!second_path.exists());
    assert_eq!(
        fs::read(first.path.join("winner-data")).unwrap(),
        b"first export"
    );
    let current = fs::metadata(&first.path).unwrap();
    assert_eq!(
        (current.dev(), current.ino()),
        (first_identity.dev(), first_identity.ino())
    );
    drop(first);
    assert!(!first_path.exists());
}

#[test]
fn exhausted_staging_candidates_preserve_occupied_entries() {
    let fixture = create().unwrap();
    let directory = fixture.path.join("directory");
    fs::create_dir(&directory).unwrap();
    fs::write(directory.join("caller-data"), b"caller directory").unwrap();
    let file = fixture.path.join("file");
    fs::write(&file, b"caller file").unwrap();
    let target = fixture.path.join("absent-target");
    let link = fixture.path.join("link");
    symlink(&target, &link).unwrap();
    for path in [&directory, &file, &link] {
        let before = fs::symlink_metadata(path).unwrap();
        let error = match reserve([path.clone(), path.clone()]) {
            Ok(_) => panic!("occupied staging candidate was admitted"),
            Err(error) => error,
        };
        assert_eq!(error.kind(), io::ErrorKind::AlreadyExists);
        let after = fs::symlink_metadata(path).unwrap();
        assert_eq!((before.dev(), before.ino()), (after.dev(), after.ino()));
    }
    assert_eq!(
        fs::read(directory.join("caller-data")).unwrap(),
        b"caller directory"
    );
    assert_eq!(fs::read(file).unwrap(), b"caller file");
    assert_eq!(fs::read_link(link).unwrap(), target);
    assert!(!target.exists());
}

#[test]
fn staging_metadata_errors_do_not_skip_to_another_candidate() {
    let fixture = create().unwrap();
    let file = fixture.path.join("caller-file");
    fs::write(&file, b"caller bytes").unwrap();
    let invalid = file.join("stage");
    let later = fixture.path.join("unused");
    let expected = fs::create_dir(&invalid).unwrap_err().kind();
    let error = match reserve([invalid, later.clone()]) {
        Ok(_) => panic!("non-collision error was ignored"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), expected);
    assert!(!later.exists());
    assert_eq!(fs::read(file).unwrap(), b"caller bytes");
}
