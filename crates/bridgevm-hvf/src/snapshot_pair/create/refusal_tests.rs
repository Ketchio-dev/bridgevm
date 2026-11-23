//! What creation refuses, and what a refusal or a failed publication keeps.

use super::*;
use crate::snapshot_pair::publish::publish_with;
use crate::snapshot_pair::tests::Scratch;
use crate::snapshot_pair::verify_snapshot;
use std::os::unix::fs::symlink;

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
    create_snapshot_using(disk, vars, dest, vm_id, |stage| {
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
    let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
    fs::write(&disk, b"newer disk").unwrap();
    fs::write(&vars, b"newer vars").unwrap();
    let new = create_snapshot(&disk, &vars, &dest, "new").unwrap();
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
    create_snapshot(&disk, &vars, &dest, "old").unwrap();
    fs::write(dest.join(DISK_NAME), b"XXXXXXXX").unwrap();
    assert!(matches!(
        verify_snapshot(&dest),
        Err(SnapshotError::HashMismatch { .. })
    ));
    let new = create_snapshot(&disk, &vars, &dest, "new").unwrap();
    assert_eq!(verify_snapshot(&dest).unwrap(), new);
    assert!(!staging_path(&dest).exists());
}

#[test]
fn an_empty_existing_directory_is_accepted() {
    let s = Scratch::new("admission-empty");
    let (disk, vars) = sources(&s);
    let dest = s.path("empty");
    fs::create_dir(&dest).unwrap();
    let created = create_snapshot(&disk, &vars, &dest, "vm").unwrap();
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
    let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
    s.write("snapshot/notes.txt", b"user notes");
    let before = tree(&dest);
    let result = create_uncopied(&disk, &vars, &dest, "new");
    assert_eq!(tree(&dest), before, "export deleted an extra entry");
    assert_refused(result, "extra entry");
    assert_eq!(verify_snapshot(&dest).unwrap(), old);
    assert!(!staging_path(&dest).exists(), "refusal left staging");
}

/// Declare each member's lstat size, so only its file type can refuse it.
fn declare_actual_sizes(dest: &Path, old: &SnapshotManifest) {
    let size = |name| fs::symlink_metadata(dest.join(name)).unwrap().len();
    let forged = SnapshotManifest {
        disk_bytes: size(DISK_NAME),
        vars_bytes: size(VARS_NAME),
        ..old.clone()
    };
    fs::write(dest.join(MANIFEST_NAME), forged.to_json()).unwrap();
}

#[test]
fn inadmissible_snapshot_shapes_are_refused_before_staging() {
    for case in [
        "missing-member",
        "subdirectory",
        "unparseable-manifest",
        "unsupported-manifest",
        "oversized-manifest",
        "disk-size-mismatch",
        "vars-size-mismatch",
        "linked-member",
        "directory-member",
        "unrelated-names",
    ] {
        let s = Scratch::new(&format!("admission-shape-{case}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
        let elsewhere = s.write("elsewhere", b"new disk");
        match case {
            "missing-member" => fs::remove_file(dest.join(VARS_NAME)).unwrap(),
            "subdirectory" => {
                fs::create_dir(dest.join("nested")).unwrap();
                fs::write(dest.join("nested/keep.txt"), b"nested data").unwrap();
            }
            "unparseable-manifest" => fs::write(dest.join(MANIFEST_NAME), b"not json").unwrap(),
            "unsupported-manifest" => {
                let text = old
                    .to_json()
                    .replace("\"format_version\": 1", "\"format_version\": 2");
                fs::write(dest.join(MANIFEST_NAME), text).unwrap();
            }
            // Still parses: only the size limit refuses it.
            "oversized-manifest" => {
                let text = old.to_json() + &" ".repeat(64 * 1024);
                fs::write(dest.join(MANIFEST_NAME), text).unwrap();
            }
            "disk-size-mismatch" => fs::write(dest.join(DISK_NAME), b"longer disk").unwrap(),
            "vars-size-mismatch" => fs::write(dest.join(VARS_NAME), b"v").unwrap(),
            "linked-member" => {
                fs::remove_file(dest.join(DISK_NAME)).unwrap();
                symlink(&elsewhere, dest.join(DISK_NAME)).unwrap();
                declare_actual_sizes(&dest, &old);
            }
            "directory-member" => {
                fs::remove_file(dest.join(VARS_NAME)).unwrap();
                fs::create_dir(dest.join(VARS_NAME)).unwrap();
                fs::write(dest.join(VARS_NAME).join("keep.txt"), b"nested data").unwrap();
                declare_actual_sizes(&dest, &old);
            }
            "unrelated-names" => {
                for name in [DISK_NAME, VARS_NAME, MANIFEST_NAME] {
                    fs::rename(dest.join(name), dest.join(format!("{name}.bak"))).unwrap();
                }
            }
            _ => unreachable!(),
        }
        let before = (tree(&dest), tree(&elsewhere));
        let result = create_uncopied(&disk, &vars, &dest, "new");
        assert_eq!((tree(&dest), tree(&elsewhere)), before, "{case}: changed");
        assert_refused(result, case);
        assert!(
            !staging_path(&dest).exists(),
            "{case}: refusal left staging"
        );
    }
}

/// exFAT keeps a written file's extended attributes in a "._" AppleDouble file
/// beside it, and its directory exchange fails with ENOTSUP, so a snapshot there
/// cannot be replaced in place anyway.
#[test]
fn a_snapshot_with_apple_double_companions_is_refused_as_not_a_snapshot() {
    let s = Scratch::new("admission-apple-double");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    create_snapshot(&disk, &vars, &dest, "old").unwrap();
    s.write("snapshot/._disk.raw", b"\0\x05\x16\x07 AppleDouble");
    let before = tree(&dest);
    let result = create_uncopied(&disk, &vars, &dest, "new");
    assert_eq!(tree(&dest), before);
    let Err(SnapshotError::Io(error)) = result else {
        panic!("expected a refusal, got {result:?}");
    };
    let expected = "contains \"._disk.raw\", which is not a snapshot file; it was left intact";
    assert!(error.to_string().contains(expected), "{error}");
}

#[test]
fn unrelated_staging_contents_are_refused_and_left_intact() {
    let s = Scratch::new("admission-staging-unrelated");
    let (disk, vars) = sources(&s);
    let dest = s.path("snapshot");
    let staging = staging_path(&dest);
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join("keep.txt"), b"unclaimed staging data").unwrap();
    let before = tree(&staging);
    let result = create_snapshot(&disk, &vars, &dest, "vm");
    assert_eq!(tree(&staging), before, "export deleted unclaimed staging");
    assert_refused(result, "unrelated staging");
    assert!(!dest.exists(), "refusal published a snapshot");
}

#[test]
fn staging_that_is_not_bridgevm_debris_is_refused_and_left_intact() {
    for case in ["symlink", "file", "nested-directory", "linked-member"] {
        let s = Scratch::new(&format!("admission-staging-{case}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        let outside = s.path("outside");
        fs::create_dir(&outside).unwrap();
        s.write("outside/keep.txt", b"outside data");
        match case {
            "symlink" => symlink(&outside, &staging).unwrap(),
            "file" => fs::write(&staging, b"a user file").unwrap(),
            "nested-directory" => {
                fs::create_dir_all(staging.join(DISK_NAME)).unwrap();
                fs::write(staging.join(DISK_NAME).join("keep.txt"), b"nested").unwrap();
            }
            "linked-member" => {
                fs::create_dir(&staging).unwrap();
                symlink(outside.join("keep.txt"), staging.join(VARS_NAME)).unwrap();
            }
            _ => unreachable!(),
        }
        let before = (tree(&staging), tree(&outside));
        let result = create_snapshot(&disk, &vars, &dest, "vm");
        assert_eq!((tree(&staging), tree(&outside)), before, "{case}: changed");
        assert_refused(result, case);
        assert!(!dest.exists(), "{case}: refusal published a snapshot");
    }
}

/// Staging holding a published manifest is kept (`completed_staging_tests`).
#[test]
fn staging_holding_only_bridgevm_debris_is_cleared_for_retry() {
    let cases: [&[&str]; 3] = [
        &[],
        &[DISK_NAME],
        &[DISK_NAME, VARS_NAME, ".manifest.json.tmp"],
    ];
    for (index, names) in cases.into_iter().enumerate() {
        let s = Scratch::new(&format!("admission-debris-{index}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        for name in names {
            fs::write(staging.join(name), b"partial").unwrap();
        }
        let created = create_snapshot(&disk, &vars, &dest, "vm").unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), created, "{names:?}");
        assert!(!staging.exists(), "{names:?}: staging was not cleared");
    }
}

/// exFAT keeps each written file's extended attributes in a "._" AppleDouble
/// file beside it, so an interrupted export there leaves those in staging too.
#[test]
fn apple_double_companions_in_staging_are_cleared_for_retry() {
    let cases: [&[&str]; 4] = [
        &["._disk.raw"],
        &[DISK_NAME, "._disk.raw"],
        &[
            VARS_NAME,
            "._vars.fd",
            ".manifest.json.tmp",
            "._.manifest.json.tmp",
        ],
        &[DISK_NAME, "._disk.raw", "._manifest.json"],
    ];
    for (index, names) in cases.into_iter().enumerate() {
        let s = Scratch::new(&format!("debris-apple-double-{index}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        for name in names {
            fs::write(staging.join(name), b"partial").unwrap();
        }
        let created = create_snapshot(&disk, &vars, &dest, "vm")
            .unwrap_or_else(|error| panic!("{names:?}: {error}"));
        assert_eq!(verify_snapshot(&dest).unwrap(), created, "{names:?}");
        assert!(!staging.exists(), "{names:?}: staging was not cleared");
    }
}

#[test]
fn staging_companions_that_are_not_debris_are_refused_and_left_intact() {
    for case in ["other-name", "linked", "directory"] {
        let s = Scratch::new(&format!("debris-not-companion-{case}"));
        let (disk, vars) = sources(&s);
        let dest = s.path("snapshot");
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        let outside = s.write("outside", b"outside data");
        match case {
            "other-name" => fs::write(staging.join("._keep.txt"), b"user data").unwrap(),
            "linked" => symlink(&outside, staging.join("._disk.raw")).unwrap(),
            "directory" => {
                fs::create_dir(staging.join("._disk.raw")).unwrap();
                fs::write(staging.join("._disk.raw/keep.txt"), b"nested").unwrap();
            }
            _ => unreachable!(),
        }
        let before = (tree(&staging), tree(&outside));
        let result = create_uncopied(&disk, &vars, &dest, "vm");
        assert_eq!((tree(&staging), tree(&outside)), before, "{case}: changed");
        assert_refused(result, case);
        assert!(!dest.exists(), "{case}: refusal published a snapshot");
    }
}

/// exFAT unlinks a file's "._" companion with it, after staging was listed.
#[test]
fn a_companion_that_left_with_its_file_is_not_an_error() {
    let s = Scratch::new("debris-companion-gone");
    let staging = s.path(".snapshot.staging");
    fs::create_dir(&staging).unwrap();
    for name in [DISK_NAME, "._disk.raw"] {
        fs::write(staging.join(name), b"partial").unwrap();
    }
    let unlink = || fs::remove_file(staging.join("._disk.raw")).unwrap();
    clear_staging_then(&staging, unlink).unwrap();
    assert!(!staging.exists());
}

#[test]
fn an_entry_that_arrives_after_listing_is_kept() {
    let s = Scratch::new("debris-late-arrival");
    let staging = s.path(".snapshot.staging");
    fs::create_dir(&staging).unwrap();
    fs::write(staging.join(DISK_NAME), b"partial").unwrap();
    let arrive = || fs::write(staging.join("keep.txt"), b"arrived after listing").unwrap();
    assert!(clear_staging_then(&staging, arrive).is_err());
    let kept = fs::read(staging.join("keep.txt")).ok();
    assert_eq!(kept.as_deref(), Some(&b"arrived after listing"[..]));
    assert!(!staging.join(DISK_NAME).exists());
}

// Publication that fails inside a real create, through publish_with's swap.

fn unsupported() -> io::Error {
    // exFAT has no directory swap.
    io::Error::from_raw_os_error(libc::ENOTSUP)
}

fn create_with(
    s: &Scratch,
    vm_id: &str,
    publish: impl FnOnce(&Path, &Path) -> io::Result<()>,
) -> Result<SnapshotManifest, SnapshotError> {
    let (disk, vars, dest) = (s.path("disk"), s.path("vars"), s.path("snapshot"));
    create_stopped(&disk, &vars, &dest, vm_id, copy_and_sync, publish, |_| {})
}

fn assert_unsupported(result: Result<SnapshotManifest, SnapshotError>) -> String {
    match result {
        Err(SnapshotError::Io(e)) if e.kind() == io::ErrorKind::Unsupported => e.to_string(),
        other => panic!("expected the typed exchange refusal, got {other:?}"),
    }
}

/// An old snapshot at the destination, so publication must swap; the sources
/// then change so a published replacement would be visible.
fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, SnapshotManifest, Vec<u8>) {
    let s = Scratch::new(tag);
    let (disk, vars, dest) = (
        s.write("disk", b"old"),
        s.write("vars", b"v"),
        s.path("snapshot"),
    );
    let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
    fs::write(&disk, b"new").unwrap();
    let manifest = fs::read(dest.join(MANIFEST_NAME)).unwrap();
    let staging = staging_path(&dest);
    (s, dest, staging, old, manifest)
}

fn assert_old_kept(dest: &Path, old: &SnapshotManifest, manifest: &[u8]) {
    assert_eq!(&verify_snapshot(dest).unwrap(), old);
    assert_eq!(fs::read(dest.join(MANIFEST_NAME)).unwrap(), manifest);
    assert_eq!(fs::read(dest.join(DISK_NAME)).unwrap(), b"old");
}

#[test]
fn a_failed_swap_clears_this_attempts_staging_and_a_retry_meets_the_same_error() {
    let (s, dest, staging, old, manifest) = fixture("publication-unsupported");
    for vm_id in ["new", "retry"] {
        let failing = |staged: &Path, current: &Path| {
            assert_eq!(verify_snapshot(staged).unwrap().vm_id, vm_id);
            publish_with(staged, current, |_, _| Err(unsupported()))
        };
        assert_unsupported(create_with(&s, vm_id, failing));
        assert!(
            !staging.exists(),
            "{vm_id}: unpublished staging was stranded"
        );
        assert_old_kept(&dest, &old, &manifest);
    }
    let created = create_snapshot(&s.path("disk"), &s.path("vars"), &dest, "vm");
    assert_eq!(verify_snapshot(&dest).unwrap(), created.unwrap());
    assert_eq!(fs::read(dest.join(DISK_NAME)).unwrap(), b"new");
}

#[test]
fn staging_replaced_before_publication_fails_is_not_cleared() {
    let (s, dest, staging, old, manifest) = fixture("publication-replaced-staging");
    let moved = s.path("moved");
    let replace = |staged: &Path, _: &Path| {
        fs::rename(staged, &moved)?;
        // Reclaimable debris by name, so only identity protects it.
        fs::create_dir(staged)?;
        fs::write(staged.join(DISK_NAME), b"foreign")?;
        Err(unsupported())
    };
    assert!(create_with(&s, "new", replace).is_err());
    assert_eq!(fs::read(staging.join(DISK_NAME)).unwrap(), b"foreign");
    assert_eq!(fs::read_dir(&staging).unwrap().count(), 1);
    assert_eq!(verify_snapshot(&moved).unwrap().vm_id, "new");
    assert_old_kept(&dest, &old, &manifest);
}

/// A swap that took effect leaves the replaced snapshot at the staging name;
/// a later failure, such as the parent sync, must not delete it.
#[test]
fn a_failure_after_the_swap_keeps_the_replaced_snapshot() {
    let (s, dest, staging, old, _) = fixture("publication-after-swap");
    let swapped_then_failed = |staged: &Path, current: &Path| {
        publish_with(staged, current, |left, right| {
            let aside = s.path("aside");
            fs::rename(right, &aside)?;
            fs::rename(left, right)?;
            fs::rename(&aside, left)?;
            Err(io::Error::other("injected failure after the swap"))
        })
    };
    assert!(create_with(&s, "new", swapped_then_failed).is_err());
    assert_eq!(verify_snapshot(&staging).unwrap(), old);
    assert_eq!(verify_snapshot(&dest).unwrap().vm_id, "new");
}

// What a failed publication reports: the unsupported exchange by name and
// volume type, the untouched old snapshot, and staging it had to keep.

const GUIDANCE: &str = "choose an output path that does not exist yet, or one on an APFS volume";

fn swap_fails_with(error: impl Fn() -> io::Error) -> impl Fn(&Path, &Path) -> io::Result<()> {
    move |staged, current| publish_with(staged, current, |_, _| Err(error()))
}

#[test]
fn an_unsupported_exchange_names_the_operation_and_volume_and_keeps_the_old_snapshot() {
    let (s, dest, staging, old, manifest) = fixture("publication-unsupported-report");
    let reported = assert_unsupported(create_with(&s, "new", swap_fails_with(unsupported)));
    // Only macOS reports the volume's type.
    let volume = if cfg!(target_os = "macos") {
        "a volume of type "
    } else {
        "a volume"
    };
    let prefix = format!("{} is on {volume}", dest.canonicalize().unwrap().display());
    let operation = format!(
        " that cannot atomically exchange directories (renamex_np RENAME_SWAP: {}); \
         it was left unchanged; {GUIDANCE}",
        unsupported()
    );
    assert!(reported.starts_with(&prefix), "{reported}");
    assert!(reported.ends_with(&operation), "{reported}");
    assert!(!staging.exists(), "cleared staging is not reported as kept");
    assert_old_kept(&dest, &old, &manifest);
}

#[test]
fn other_publication_errors_keep_their_os_error_once_staging_is_cleared() {
    for errno in [libc::EXDEV, libc::EIO] {
        let (s, dest, staging, old, manifest) = fixture(&format!("publication-errno-{errno}"));
        let failing = swap_fails_with(|| io::Error::from_raw_os_error(errno));
        match create_with(&s, "new", failing) {
            Err(SnapshotError::Io(error)) => assert_eq!(error.raw_os_error(), Some(errno)),
            other => panic!("expected the swap's own error, got {other:?}"),
        }
        assert!(!staging.exists());
        assert_old_kept(&dest, &old, &manifest);
    }
}

#[test]
fn staging_that_cannot_be_cleared_is_named_with_the_reason() {
    let (s, dest, staging, old, manifest) = fixture("publication-uncleared");
    let failing = |staged: &Path, current: &Path| {
        fs::write(staged.join("late.txt"), b"late")?;
        swap_fails_with(unsupported)(staged, current)
    };
    let reported = assert_unsupported(create_with(&s, "new", failing));
    let kept = format!(
        "{GUIDANCE}; this attempt's staging {} was kept: it could not be cleared (",
        staging.canonicalize().unwrap().display()
    );
    assert!(reported.contains(&kept), "{reported}");
    assert!(reported.contains("late.txt"), "{reported}");
    assert_eq!(fs::read(staging.join("late.txt")).unwrap(), b"late");
    assert_eq!(fs::read(staging.join(DISK_NAME)).unwrap(), b"new");
    assert_old_kept(&dest, &old, &manifest);
}

#[test]
fn staging_replaced_before_the_failure_is_named_as_another_directory() {
    let (s, dest, staging, old, manifest) = fixture("publication-replaced-report");
    let replace = |staged: &Path, current: &Path| {
        publish_with(staged, current, |staged, _| {
            fs::rename(staged, s.path("moved"))?;
            fs::create_dir(staged)?;
            Err(unsupported())
        })
    };
    let reported = assert_unsupported(create_with(&s, "new", replace));
    let kept = format!(
        "; this attempt's staging {} was kept: the path now names another directory",
        staging.canonicalize().unwrap().display()
    );
    assert!(reported.contains(&kept), "{reported}");
    assert!(staging.is_dir() && fs::read_dir(&staging).unwrap().next().is_none());
    assert_old_kept(&dest, &old, &manifest);
}
