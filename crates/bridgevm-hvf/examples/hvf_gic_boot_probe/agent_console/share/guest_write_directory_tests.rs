//! Directories a guest path causes to be created (follow-up of 2026-10-23,
//! conservative choice): owner-only 0700 before the umask and marked like guest
//! files, through the new directory's descriptor, before any guest file is
//! published in it. A directory whose mark cannot be set is removed again and
//! the write is refused. A directory that already exists is used as it is.

use super::*;

/// From now on every directory created in `dir` inherits "deny writeextattr".
fn deny_directory_marks(dir: &Path) {
    let status = std::process::Command::new("/bin/chmod")
        .args(["+a", "everyone deny writeextattr,directory_inherit"])
        .arg(dir)
        .status()
        .unwrap();
    assert!(status.success());
}

#[test]
fn directories_created_for_a_guest_path_are_owner_only_and_marked() {
    let f = Fixture::new();
    let start = now();
    let d = Destination::open(&f.root(), "sub/deeper/a.txt", true).unwrap();
    d.write(b"guest").unwrap();
    let mut uuids = Vec::new();
    for level in ["sub", "sub/deeper"] {
        let path = f.root().join(level);
        let created = mode(&path);
        assert_eq!(created, 0o700, "{level} is {created:o}");
        let mark = xattr(&path, QUARANTINE).expect("unmarked guest directory");
        let (seconds, uuid) = parse_mark(&mark);
        assert!((start..=now()).contains(&seconds), "{seconds:x}");
        uuids.push(uuid);
    }
    assert_ne!(uuids[0], uuids[1], "quarantine UUID repeated");
    // The file in them keeps the file policy.
    assert_eq!(mode(&f.root().join("sub/deeper/a.txt")), 0o600);
}

#[test]
fn an_existing_directory_is_used_as_it_is() {
    let f = Fixture::new();
    let sub = f.root().join("sub");
    std::fs::create_dir(&sub).unwrap();
    std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o755)).unwrap();
    let d = Destination::open(&f.root(), "sub/a.txt", true).unwrap();
    d.write(b"guest").unwrap();
    assert_eq!(mode(&sub), 0o755);
    assert_eq!(
        xattr(&sub, QUARANTINE),
        None,
        "an existing directory was marked"
    );
    parse_mark(&xattr(&sub.join("a.txt"), QUARANTINE).expect("unmarked guest file"));
}

#[test]
fn a_directory_whose_mark_cannot_be_set_is_removed_and_nothing_is_written() {
    let f = Fixture::new();
    deny_directory_marks(&f.root());
    let written = Destination::open(&f.root(), "sub/a.txt", true).and_then(|d| d.write(b"guest"));
    assert!(
        written.is_err(),
        "a guest file was published in an unmarked directory"
    );
    let left: Vec<_> = std::fs::read_dir(f.root()).unwrap().collect();
    assert!(left.is_empty(), "{} entries left behind", left.len());
}

#[test]
fn a_refused_directory_mark_holds_until_the_guest_file_changes() {
    use crate::agent_console::share::guest_write_refusal::is_persistent;
    let f = Fixture::new();
    deny_directory_marks(&f.root());
    let refused = Destination::open(&f.root(), "sub/a.txt", true).err();
    let error = refused.expect("opened a guest path under an unmarked directory");
    assert!(is_persistent(&error), "{error}");
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
}
