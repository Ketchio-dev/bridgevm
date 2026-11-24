use super::files::COPY_CHUNK;
use super::*;
use std::ffi::OsString;

/// A scratch directory that removes itself.
pub(super) struct Scratch(pub(super) PathBuf);

impl Scratch {
    pub(super) fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bv-snap-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        Self(dir)
    }
    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    pub(super) fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let p = self.path(name);
        fs::write(&p, bytes).expect("write");
        p
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn a_snapshot_records_the_hash_of_both_files() {
    let s = Scratch::new("hashes");
    let disk = s.write("disk-src", b"disk contents");
    let vars = s.write("vars-src", b"vars contents");
    let m = create_snapshot(&disk, &vars, &s.path("snap"), "vm-1").expect("create");

    assert_eq!(m.disk_bytes, 13);
    assert_eq!(m.vars_bytes, 13);
    assert_eq!(m.disk_sha256.len(), 64);
    assert_ne!(m.disk_sha256, m.vars_sha256);
    assert_eq!(m.vm_id, "vm-1");
    assert_eq!(m.format_version, SNAPSHOT_FORMAT_VERSION);
}

#[test]
fn verify_accepts_a_snapshot_it_just_created() {
    let s = Scratch::new("verify-ok");
    let disk = s.write("disk-src", b"alpha");
    let vars = s.write("vars-src", b"beta");
    let created = create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    let verified = verify_snapshot(&s.path("snap")).expect("verify");
    assert_eq!(created, verified);
}

#[test]
fn verify_rejects_a_snapshot_whose_disk_was_altered() {
    let s = Scratch::new("verify-disk");
    let disk = s.write("disk-src", b"alpha");
    let vars = s.write("vars-src", b"beta");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();

    fs::write(s.path("snap").join(DISK_NAME), b"ALTER").unwrap();
    match verify_snapshot(&s.path("snap")) {
        Err(SnapshotError::HashMismatch { file }) => assert_eq!(file, DISK_NAME),
        other => panic!("a tampered disk must not verify, got {other:?}"),
    }
}

#[test]
fn verify_rejects_a_snapshot_whose_vars_were_altered() {
    let s = Scratch::new("verify-vars");
    let disk = s.write("disk-src", b"alpha");
    let vars = s.write("vars-src", b"beta");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();

    fs::write(s.path("snap").join(VARS_NAME), b"EDIT").unwrap();
    match verify_snapshot(&s.path("snap")) {
        Err(SnapshotError::HashMismatch { file }) => assert_eq!(file, VARS_NAME),
        other => panic!("tampered vars must not verify, got {other:?}"),
    }
}

#[test]
fn restore_puts_back_exactly_what_was_captured() {
    let s = Scratch::new("restore");
    let disk = s.write("disk", b"original disk");
    let vars = s.write("vars", b"original vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    fs::write(&disk, b"changed since").unwrap();
    fs::write(&vars, b"also changed").unwrap();
    restore_snapshot(&s.path("snap"), &disk, &vars).expect("restore");
    let pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let (disk, vars) = pair.paths().unwrap();
    assert_eq!(fs::read(&disk).unwrap(), b"original disk");
    assert_eq!(fs::read(&vars).unwrap(), b"original vars");
}

#[test]
fn restore_refuses_a_tampered_snapshot_without_touching_the_live_pair() {
    let s = Scratch::new("restore-guard");
    let disk = s.write("disk", b"live disk");
    let vars = s.write("vars", b"live vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    fs::write(s.path("snap").join(VARS_NAME), b"tampered").unwrap();

    assert!(restore_snapshot(&s.path("snap"), &disk, &vars).is_err());
    // This is the point of verifying first: the user still has what they had.
    assert_eq!(fs::read(&disk).unwrap(), b"live disk");
    assert_eq!(fs::read(&vars).unwrap(), b"live vars");
}

fn share_state(vars: &Path, suffix: &str) -> PathBuf {
    let mut path = vars.as_os_str().to_owned();
    path.push(suffix);
    PathBuf::from(path)
}

#[test]
fn restore_removes_the_share_state_beside_the_vars() {
    let s = Scratch::new("restore-share-state");
    let disk = s.write("disk", b"original disk");
    let vars = s.write("vars", b"original vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    for suffix in SHARE_STATE_SUFFIXES {
        fs::write(share_state(&vars, suffix), b"synced since the snapshot").unwrap();
    }
    restore_snapshot(&s.path("snap"), &disk, &vars).expect("restore");
    for suffix in SHARE_STATE_SUFFIXES {
        assert!(
            !share_state(&vars, suffix).exists(),
            "{suffix} outlived the restore"
        );
    }
}

#[test]
fn a_share_state_that_cannot_be_removed_fails_the_restore_before_the_pair_changes() {
    let s = Scratch::new("restore-share-state-stuck");
    let disk = s.write("disk", b"original disk");
    let vars = s.write("vars", b"original vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    fs::write(&disk, b"changed since").unwrap();
    fs::write(&vars, b"also changed").unwrap();
    fs::create_dir(share_state(&vars, SHARE_STATE_SUFFIXES[1])).unwrap();

    assert!(restore_snapshot(&s.path("snap"), &disk, &vars).is_err());
    let pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let (disk, vars) = pair.paths().unwrap();
    assert_eq!(fs::read(&disk).unwrap(), b"changed since");
    assert_eq!(fs::read(&vars).unwrap(), b"also changed");
}

#[test]
fn an_interrupted_create_leaves_the_previous_snapshot_intact() {
    let s = Scratch::new("interrupted");
    let disk = s.write("disk", b"first");
    let vars = s.write("vars", b"first vars");
    let dest = s.path("snap");
    let first = create_snapshot(&disk, &vars, &dest, "vm").unwrap();

    // Simulate a crash partway through a second snapshot: a staging directory
    // exists with a half-written disk and no manifest.
    let staging = staging_path(&dest);
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join(DISK_NAME), b"partial").unwrap();

    // The published snapshot is untouched and still verifies.
    assert_eq!(verify_snapshot(&dest).unwrap(), first);

    // And the next create clears the debris rather than mixing with it.
    fs::write(&disk, b"second").unwrap();
    let second = create_snapshot(&disk, &vars, &dest, "vm").unwrap();
    assert_ne!(second.disk_sha256, first.disk_sha256);
    assert_eq!(verify_snapshot(&dest).unwrap(), second);
}

#[test]
fn a_snapshot_directory_without_a_manifest_is_not_mistaken_for_one() {
    let s = Scratch::new("no-manifest");
    let dir = s.path("snap");
    fs::create_dir_all(&dir).unwrap();
    fs::write(dir.join(DISK_NAME), b"d").unwrap();
    fs::write(dir.join(VARS_NAME), b"v").unwrap();

    assert!(matches!(
        verify_snapshot(&dir),
        Err(SnapshotError::BadManifest(_))
    ));
}

#[test]
fn a_manifest_from_a_future_format_is_refused_rather_than_guessed_at() {
    let s = Scratch::new("future");
    let dir = s.path("snap");
    fs::create_dir_all(&dir).unwrap();
    let future = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION + 1,
        vm_id: "vm".into(),
        disk_bytes: 1,
        disk_sha256: "00".into(),
        vars_bytes: 1,
        vars_sha256: "00".into(),
    };
    fs::write(dir.join(MANIFEST_NAME), future.to_json()).unwrap();

    match verify_snapshot(&dir) {
        Err(SnapshotError::BadManifest(why)) => assert!(why.contains("format version")),
        other => panic!("expected a version refusal, got {other:?}"),
    }
}

#[test]
fn an_atomic_write_replaces_the_old_content_completely() {
    let s = Scratch::new("atomic");
    let p = s.write("f", b"old content that is longer");
    write_file_atomically(&p, b"new").unwrap();
    assert_eq!(fs::read(&p).unwrap(), b"new");
    // No temp file is left behind.
    let leftovers: Vec<_> = fs::read_dir(&s.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "temp file left behind");
}

#[test]
fn a_large_file_hashes_the_same_as_its_bytes() {
    let s = Scratch::new("bigger-than-chunk");
    // Larger than COPY_CHUNK so the streaming path takes more than one lap.
    let bytes: Vec<u8> = (0..(COPY_CHUNK + 4096)).map(|i| (i % 251) as u8).collect();
    let disk = s.write("disk", &bytes);
    let vars = s.write("vars", b"v");
    let m = create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();

    assert_eq!(m.disk_bytes, bytes.len() as u64);
    assert_eq!(fs::read(s.path("snap").join(DISK_NAME)).unwrap(), bytes);
    verify_snapshot(&s.path("snap")).expect("a multi-chunk copy must verify");
}

// Snapshot copy independence and refusal cleanup.

#[test]
fn a_failed_restore_leaves_no_staging_file_behind() {
    let s = Scratch::new("no-debris");
    let disk = s.write("disk", b"live disk");
    let vars = s.write("vars", b"live vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm").unwrap();
    // Remove the snapshot's disk so the staging copy fails after verification
    // would have passed.
    fs::remove_file(s.path("snap").join(DISK_NAME)).unwrap();

    assert!(restore_snapshot(&s.path("snap"), &disk, &vars).is_err());
    let debris: Vec<_> = fs::read_dir(&s.0)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().ends_with(".restore"))
        .collect();
    assert!(
        debris.is_empty(),
        "a failed restore left staging files behind"
    );
}

#[test]
fn a_clone_reproduces_the_bytes_and_an_independent_write_does_not_disturb_it() {
    // Copy-on-write is only safe here because the clone stops sharing the
    // moment either side is written. If it did not, restoring a snapshot and
    // then booting the VM would rewrite the snapshot too.
    let s = Scratch::new("clone");
    let src = s.write("src", b"original bytes");
    let dst = s.path("dst");
    if files::clone_file(&src, &dst).is_none() {
        return; // not APFS; the copy path covers this case
    }
    assert_eq!(fs::read(&dst).unwrap(), b"original bytes");
    fs::write(&dst, b"rewritten!!!!!").unwrap();
    assert_eq!(fs::read(&src).unwrap(), b"original bytes");
}

#[test]
fn cloning_refuses_rather_than_overwriting_an_existing_file() {
    let s = Scratch::new("clone-exists");
    let src = s.write("src", b"source");
    let dst = s.write("dst", b"must survive");
    assert!(files::clone_file(&src, &dst).is_none());
    assert_eq!(fs::read(&dst).unwrap(), b"must survive");
}

#[test]
fn incorrect_declared_sizes_do_not_verify_or_replace_the_live_pair() {
    for (field, bytes) in [
        ("disk_bytes", 0),
        ("disk_bytes", 99),
        ("vars_bytes", 0),
        ("vars_bytes", 99),
    ] {
        let s = Scratch::new(&format!("declared-{field}-{bytes}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let snapshot = s.path("snapshot");
        let manifest = create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        let changed = manifest
            .to_json()
            .replace(&format!("\"{field}\": 4"), &format!("\"{field}\": {bytes}"));
        assert_ne!(changed, manifest.to_json());
        fs::write(snapshot.join(MANIFEST_NAME), changed).unwrap();
        fs::write(&disk, b"live disk").unwrap();
        fs::write(&vars, b"live vars").unwrap();

        assert!(
            matches!(
                verify_snapshot(&snapshot),
                Err(SnapshotError::BadManifest(_))
            ),
            "{field}={bytes} was accepted"
        );
        assert!(matches!(
            restore_snapshot(&snapshot, &disk, &vars),
            Err(SnapshotError::BadManifest(_))
        ));
        let pair = managed::LockedPair::open(&disk, &vars).unwrap();
        assert_eq!(
            pair.paths().unwrap(),
            (disk.canonicalize().unwrap(), vars.canonicalize().unwrap())
        );
        assert_eq!(fs::read(disk).unwrap(), b"live disk");
        assert_eq!(fs::read(vars).unwrap(), b"live vars");
    }
}

#[test]
fn a_full_width_format_version_cannot_wrap_to_the_supported_version() {
    let manifest = SnapshotManifest {
        format_version: SNAPSHOT_FORMAT_VERSION,
        vm_id: "vm".into(),
        disk_bytes: 4,
        disk_sha256: "a".repeat(64),
        vars_bytes: 4,
        vars_sha256: "b".repeat(64),
    };
    for version in [u64::from(u32::MAX) + 2, u64::MAX] {
        let text = manifest.to_json().replace(
            "\"format_version\": 1",
            &format!("\"format_version\": {version}"),
        );
        assert!(
            matches!(
                SnapshotManifest::from_json(&text),
                Err(SnapshotError::BadManifest(_))
            ),
            "unsupported version {version} was accepted"
        );
    }
}

#[test]
#[cfg(target_os = "macos")]
fn nearly_full_volume_can_restore_by_cloning() {
    let s = Scratch::new("clone-nospace");
    let disk = s.write("disk", b"disk");
    let vars = s.write("vars", b"vars");
    let snapshot = s.path("snap");
    create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
    let mut pair = managed::LockedPair::open(&disk, &vars).unwrap();
    let mut sampled = false;
    pair.restore_copying(
        &snapshot,
        |src, dst| {
            files::copy_and_sync_using(src, dst, files::clone_file, |_| {
                sampled = true;
                Some(0)
            })
        },
        publish::publish,
    )
    .unwrap();
    assert!(!sampled, "successful clone must not sample free space");
    let (disk, vars) = pair.paths().unwrap();
    assert_eq!(fs::read(disk).unwrap(), b"disk");
    assert_eq!(fs::read(vars).unwrap(), b"vars");
}

#[test]
fn failed_clone_with_short_space_refuses_before_copy_and_preserves_pair() {
    for selected in [false, true] {
        let s = Scratch::new(&format!("fallback-nospace-{selected}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let snapshot = s.path("snap");
        create_snapshot(&disk, &vars, &snapshot, "vm").unwrap();
        let mut pair = managed::LockedPair::open(&disk, &vars).unwrap();
        if selected {
            pair.restore(&snapshot).unwrap();
        }
        let paths = pair.paths().unwrap();
        fs::write(&paths.0, b"live disk").unwrap();
        fs::write(&paths.1, b"live vars").unwrap();
        let mut destination = None;
        let result = pair.restore_copying(
            &snapshot,
            |src, dst| {
                destination = Some(dst.to_path_buf());
                let result = files::copy_and_sync_using(
                    src,
                    dst,
                    |_, _| None,
                    |parent| {
                        assert_eq!(parent, dst.parent().unwrap());
                        Some(0)
                    },
                );
                assert!(
                    !dst.exists(),
                    "byte copy must not start when space is short"
                );
                result
            },
            |_, _| panic!("short-space copy reached publication"),
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "No space left on device. Free space on the destination volume and try again."
        );
        assert!(
            !destination.unwrap().parent().unwrap().exists(),
            "staging not cleared"
        );
        assert_eq!(pair.paths().unwrap(), paths);
        assert_eq!(fs::read(paths.0).unwrap(), b"live disk");
        assert_eq!(fs::read(paths.1).unwrap(), b"live vars");
    }
}

#[test]
fn copy_out_of_space_has_one_plain_sentence() {
    let error = SnapshotError::from(std::io::Error::from_raw_os_error(libc::ENOSPC));
    assert_eq!(
        error.to_string(),
        "No space left on device. Free space on the destination volume and try again."
    );
}

// Staging holding a published manifest may be a completed snapshot, such as a
// user's export to `.NAME.staging`, so creation refuses rather than reclaim it.

type Files = Option<Vec<(OsString, Vec<u8>)>>;

/// A directory's regular files and their bytes, or None when it is absent.
fn files(dir: &Path) -> Files {
    let mut files: Vec<_> = fs::read_dir(dir)
        .ok()?
        .map(|entry| {
            let entry = entry.unwrap();
            (entry.file_name(), fs::read(entry.path()).unwrap())
        })
        .collect();
    files.sort();
    Some(files)
}

fn assert_kept_and_named(result: Result<SnapshotManifest, SnapshotError>, staging: &Path) {
    let text = match result {
        Err(SnapshotError::Io(error)) => error.to_string(),
        other => panic!("expected a completed-staging refusal, got {other:?}"),
    };
    // Creation resolves the parent, so a linked TMPDIR is named canonically.
    let parent = fs::canonicalize(staging.parent().unwrap()).unwrap();
    let canonical = parent.join(staging.file_name().unwrap());
    let named = [staging, &canonical].map(|path| text.contains(&*path.to_string_lossy()));
    assert!(
        named.contains(&true),
        "refusal names neither {staging:?} nor {canonical:?}: {text}"
    );
    for wanted in ["holds a published manifest.json", "remove or rename it"] {
        assert!(text.contains(wanted), "refusal lacks {wanted:?}: {text}");
    }
}

/// Creation refuses while staging holds a published manifest and leaves both
/// outputs byte-identical; the user then removes staging as the refusal says.
pub fn refused_then_removed(disk: &Path, vars: &Path, dest: &Path) {
    let staging = staging_path(dest);
    let before = (files(&staging), files(dest));
    let result = create_snapshot(disk, vars, dest, "refused");
    let after = (files(&staging), files(dest));
    assert_eq!(after, before, "refusal changed an output");
    assert_kept_and_named(result, &staging);
    fs::remove_dir_all(&staging).unwrap();
}

/// Retry after an interruption. A `finished` attempt left a verified snapshot
/// in staging, which refuses the retry until the user removes it; a retry over
/// unfinished debris reclaims it directly.
pub fn retry_create(disk: &Path, vars: &Path, dest: &Path, finished: bool) -> SnapshotManifest {
    if finished {
        verify_snapshot(&staging_path(dest)).expect("finished staging verifies");
        refused_then_removed(disk, vars, dest);
    }
    let retried = create_snapshot(disk, vars, dest, "retry").expect("retry");
    assert!(!staging_path(dest).exists(), "retry left staging");
    retried
}

#[test]
fn an_export_at_a_sibling_staging_name_survives_the_sibling_export() {
    let s = Scratch::new("completed-staging-sibling");
    let disk = s.write("disk", b"AAAA");
    let vars = s.write("vars", b"VVVV");
    let first = s.path(".旅行.snapshot.staging");
    let second = s.path("旅行.snapshot");
    let exported = create_snapshot(&disk, &vars, &first, "vm").unwrap();
    let before = files(&first);
    fs::write(&disk, b"BBBB").unwrap();
    let result = create_snapshot(&disk, &vars, &second, "vm");
    assert_eq!(
        files(&first),
        before,
        "the completed export changed: {result:?}"
    );
    assert_eq!(verify_snapshot(&first).unwrap(), exported);
    assert_kept_and_named(result, &first);
    assert!(!second.exists(), "refusal published a snapshot");
    // Once the user moves the earlier export aside, the export proceeds.
    let moved = s.path("旅行-earlier.snapshot");
    fs::rename(&first, &moved).unwrap();
    let created = create_snapshot(&disk, &vars, &second, "vm").unwrap();
    assert_eq!(verify_snapshot(&second).unwrap(), created);
    assert_eq!(fs::read(second.join(DISK_NAME)).unwrap(), b"BBBB");
    assert_eq!(
        (verify_snapshot(&moved).unwrap(), files(&moved)),
        (exported, before)
    );
    assert!(!first.exists());
}

/// Validity is not consulted: a published manifest alone keeps the directory,
/// including with exFAT's "._" companions beside its files.
#[test]
fn staging_with_a_published_manifest_is_kept_with_the_destination() {
    let cases: [&[&str]; 3] = [
        &[DISK_NAME, VARS_NAME, MANIFEST_NAME],
        &[MANIFEST_NAME],
        &[DISK_NAME, "._disk.raw", MANIFEST_NAME, "._manifest.json"],
    ];
    for (index, names) in cases.into_iter().enumerate() {
        let s = Scratch::new(&format!("completed-staging-{index}"));
        let disk = s.write("disk", b"disk");
        let vars = s.write("vars", b"vars");
        let dest = s.path("snapshot");
        let old = create_snapshot(&disk, &vars, &dest, "old").unwrap();
        let staging = staging_path(&dest);
        fs::create_dir(&staging).unwrap();
        for name in names {
            fs::write(staging.join(name), b"finished").unwrap();
        }
        refused_then_removed(&disk, &vars, &dest);
        assert_eq!(verify_snapshot(&dest).unwrap(), old, "{names:?}");
        let new = create_snapshot(&disk, &vars, &dest, "new").unwrap();
        assert_eq!(verify_snapshot(&dest).unwrap(), new, "{names:?}");
        assert!(!staging.exists(), "{names:?}");
    }
}
