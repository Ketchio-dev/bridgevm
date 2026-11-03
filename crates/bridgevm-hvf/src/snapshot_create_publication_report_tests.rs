//! What a failed publication reports: the unsupported exchange by name and
//! volume type, the untouched old snapshot, and staging it had to keep.

use super::*;

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
