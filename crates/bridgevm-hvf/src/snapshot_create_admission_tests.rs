use crate::snapshot_pair::creation::create_snapshot_using;
use crate::snapshot_pair::snapshot_pair_tests::{Scratch, QUOTA};
use crate::snapshot_pair::{
    create_snapshot, staging_path, verify_snapshot, SnapshotError, SnapshotManifest, DISK_NAME,
    MANIFEST_NAME, VARS_NAME,
};
use std::fs;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

type Tree = Vec<(String, &'static str, Vec<u8>)>;

/// Everything at and below `path`, without following symbolic links.
fn tree(path: &Path) -> Tree {
    fn walk(path: &Path, name: String, out: &mut Tree) {
        let kind = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata.file_type(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                out.push((name, "missing", Vec::new()));
                return;
            }
            Err(error) => panic!("inspect {}: {error}", path.display()),
        };
        if kind.is_symlink() {
            let target = fs::read_link(path).unwrap();
            out.push((name, "link", target.as_os_str().as_bytes().to_vec()));
        } else if kind.is_dir() {
            out.push((name.clone(), "directory", Vec::new()));
            for entry in fs::read_dir(path).unwrap() {
                let entry = entry.unwrap();
                let child = format!("{name}/{}", entry.file_name().to_string_lossy());
                walk(&entry.path(), child, out);
            }
        } else {
            out.push((name, "file", fs::read(path).unwrap()));
        }
    }
    let mut out = Vec::new();
    walk(path, String::new(), &mut out);
    out.sort();
    out
}

fn sources(s: &Scratch) -> (PathBuf, PathBuf) {
    (
        s.write("source-disk", b"new disk"),
        s.write("source-vars", b"new vars"),
    )
}

/// Fails the test on the first copy: refusal must precede staging rather than
/// wait for the pre-publication re-check.
fn create_uncopied(
    disk: &Path,
    vars: &Path,
    dest: &Path,
    vm_id: &str,
) -> Result<SnapshotManifest, SnapshotError> {
    create_snapshot_using(disk, vars, dest, vm_id, false, QUOTA, |stage| {
        panic!("refused only after {stage:?}")
    })
}

fn assert_refused(result: Result<SnapshotManifest, SnapshotError>, case: &str) {
    match result {
        Err(SnapshotError::Io(error)) => {
            let text = error.to_string();
            assert!(
                text.contains("left intact: remove or rename it, or choose a new output path"),
                "{case}: refusal does not say the path was kept: {text}"
            );
        }
        other => panic!("{case}: expected an intact-output refusal, got {other:?}"),
    }
}

#[test]
fn an_unrelated_directory_is_refused_and_left_byte_identical() {
    let s = Scratch::new("admission-unrelated");
    let (disk, vars) = sources(&s);
    let dest = s.path("personal");
    fs::create_dir(&dest).unwrap();
    s.write("personal/keep.txt", b"irreplaceable non-VM data");
    let before = tree(&dest);
    let result = create_uncopied(&disk, &vars, &dest, "vm");
    assert_eq!(tree(&dest), before, "export deleted an unrelated directory");
    assert_refused(result, "unrelated directory");
    assert!(!staging_path(&dest).exists(), "refusal left staging");
}

#[test]
fn a_previous_snapshot_at_the_same_destination_is_replaced() {
    let s = Scratch::new("admission-replace");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    let old = create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
    fs::write(&disk, b"newer disk").unwrap();
    fs::write(&vars, b"newer vars").unwrap();
    let new = create_snapshot(&disk, &vars, &dest, "new", false, QUOTA).unwrap();
    assert_ne!(new, old);
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert_eq!(fs::read(dest.join(DISK_NAME)).unwrap(), b"newer disk");
    assert_eq!(fs::read(dest.join(VARS_NAME)).unwrap(), b"newer vars");
    assert_eq!(fs::read_dir(&dest).unwrap().count(), 3);
    assert!(!staging_path(&dest).exists());
}

#[test]
fn a_previous_snapshot_with_corrupt_bytes_of_the_declared_size_is_replaced() {
    let s = Scratch::new("admission-replace-corrupt");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
    fs::write(dest.join(DISK_NAME), b"XXXXXXXX").unwrap();
    assert!(matches!(
        verify_snapshot(&dest),
        Err(SnapshotError::HashMismatch { .. })
    ));
    let new = create_snapshot(&disk, &vars, &dest, "new", false, QUOTA).unwrap();
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert!(!staging_path(&dest).exists());
}

#[test]
fn an_empty_existing_directory_is_accepted() {
    let s = Scratch::new("admission-empty");
    let (disk, vars) = sources(&s);
    let dest = s.path("empty");
    fs::create_dir(&dest).unwrap();
    let created = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA).unwrap();
    assert_eq!(verify_snapshot(&dest).unwrap(), created);
    assert!(!staging_path(&dest).exists());
}

#[test]
fn a_symlink_destination_is_refused_before_staging() {
    let s = Scratch::new("admission-symlink");
    let (disk, vars) = sources(&s);
    fs::create_dir(s.path("target")).unwrap();
    s.write("target/keep.txt", b"linked data");
    let dest = s.path("snapshot");
    symlink(s.path("target"), &dest).unwrap();
    let before = (tree(&dest), tree(&s.path("target")));
    let result = create_uncopied(&disk, &vars, &dest, "vm");
    assert_eq!((tree(&dest), tree(&s.path("target"))), before);
    assert_refused(result, "symlink destination");
    assert!(!staging_path(&dest).exists(), "refusal left staging");
}

#[test]
fn a_file_destination_is_refused_before_staging() {
    let s = Scratch::new("admission-file");
    let (disk, vars) = sources(&s);
    let dest = s.write("snapshot", b"a user file");
    let result = create_uncopied(&disk, &vars, &dest, "vm");
    assert_eq!(fs::read(&dest).unwrap(), b"a user file");
    assert_refused(result, "file destination");
    assert!(!staging_path(&dest).exists(), "refusal left staging");
}

#[test]
fn a_previous_snapshot_with_an_extra_entry_is_refused_and_left_intact() {
    let s = Scratch::new("admission-extra");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    let old = create_snapshot(&disk, &vars, &dest, "old", false, QUOTA).unwrap();
    s.write("snapshot/notes.txt", b"user notes");
    let before = tree(&dest);
    let result = create_uncopied(&disk, &vars, &dest, "new");
    assert_eq!(tree(&dest), before, "export deleted an extra entry");
    assert_refused(result, "extra entry");
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
    assert!(!staging_path(&dest).exists(), "refusal left staging");
}

#[path = "snapshot_create_admission_shape_tests.rs"]
mod shape_tests;
#[path = "snapshot_create_staging_admission_tests.rs"]
mod staging_admission_tests;
#[path = "snapshot_create_staging_debris_tests.rs"]
mod staging_debris_tests;
