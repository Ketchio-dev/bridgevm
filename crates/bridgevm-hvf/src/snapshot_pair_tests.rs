use super::*;

/// A scratch directory that removes itself.
struct Scratch(PathBuf);

impl Scratch {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bv-snap-{}-{}", tag, std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("scratch dir");
        Self(dir)
    }
    fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
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

const QUOTA: u64 = 1024 * 1024;

#[test]
fn a_snapshot_records_the_hash_of_both_files() {
    let s = Scratch::new("hashes");
    let disk = s.write("disk-src", b"disk contents");
    let vars = s.write("vars-src", b"vars contents");
    let m = create_snapshot(&disk, &vars, &s.path("snap"), "vm-1", false, QUOTA).expect("create");

    assert_eq!(m.disk_bytes, 13);
    assert_eq!(m.vars_bytes, 13);
    assert_eq!(m.disk_sha256.len(), 64);
    assert_ne!(m.disk_sha256, m.vars_sha256);
    assert_eq!(m.vm_id, "vm-1");
    assert_eq!(m.format_version, SNAPSHOT_FORMAT_VERSION);
}

#[test]
fn a_running_vm_is_refused_before_anything_is_written() {
    let s = Scratch::new("running");
    let disk = s.write("disk-src", b"d");
    let vars = s.write("vars-src", b"v");
    let dest = s.path("snap");
    let err = create_snapshot(&disk, &vars, &dest, "vm", true, QUOTA).unwrap_err();

    assert!(matches!(err, SnapshotError::VmRunning));
    assert!(
        !dest.exists(),
        "a refused snapshot must not leave a directory"
    );
}

#[test]
fn a_pair_over_the_quota_is_refused_before_any_bytes_are_copied() {
    let s = Scratch::new("quota");
    let disk = s.write("disk-src", &[0u8; 900]);
    let vars = s.write("vars-src", &[0u8; 200]);
    let dest = s.path("snap");
    let err = create_snapshot(&disk, &vars, &dest, "vm", false, 1000).unwrap_err();

    match err {
        SnapshotError::QuotaExceeded { bytes, quota } => {
            assert_eq!(bytes, 1100);
            assert_eq!(quota, 1000);
        }
        other => panic!("expected QuotaExceeded, got {other:?}"),
    }
    assert!(!dest.exists());
}

#[test]
fn a_snapshot_exactly_at_the_quota_is_allowed() {
    let s = Scratch::new("quota-edge");
    let disk = s.write("disk-src", &[7u8; 600]);
    let vars = s.write("vars-src", &[8u8; 400]);
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, 1000).expect("at quota");
}

#[test]
fn verify_accepts_a_snapshot_it_just_created() {
    let s = Scratch::new("verify-ok");
    let disk = s.write("disk-src", b"alpha");
    let vars = s.write("vars-src", b"beta");
    let created = create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();
    let verified = verify_snapshot(&s.path("snap")).expect("verify");
    assert_eq!(created, verified);
}

#[test]
fn verify_rejects_a_snapshot_whose_disk_was_altered() {
    let s = Scratch::new("verify-disk");
    let disk = s.write("disk-src", b"alpha");
    let vars = s.write("vars-src", b"beta");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();

    fs::write(s.path("snap").join(DISK_NAME), b"tampered").unwrap();
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
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();

    fs::write(s.path("snap").join(VARS_NAME), b"tampered").unwrap();
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
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();

    fs::write(&disk, b"changed since").unwrap();
    fs::write(&vars, b"also changed").unwrap();

    restore_snapshot(&s.path("snap"), &disk, &vars, false).expect("restore");
    assert_eq!(fs::read(&disk).unwrap(), b"original disk");
    assert_eq!(fs::read(&vars).unwrap(), b"original vars");
}

#[test]
fn restore_refuses_a_tampered_snapshot_without_touching_the_live_pair() {
    let s = Scratch::new("restore-guard");
    let disk = s.write("disk", b"live disk");
    let vars = s.write("vars", b"live vars");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();
    fs::write(s.path("snap").join(VARS_NAME), b"tampered").unwrap();

    assert!(restore_snapshot(&s.path("snap"), &disk, &vars, false).is_err());
    // This is the point of verifying first: the user still has what they had.
    assert_eq!(fs::read(&disk).unwrap(), b"live disk");
    assert_eq!(fs::read(&vars).unwrap(), b"live vars");
}

#[test]
fn restore_refuses_while_the_vm_is_running() {
    let s = Scratch::new("restore-running");
    let disk = s.write("disk", b"live");
    let vars = s.write("vars", b"live");
    create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, QUOTA).unwrap();

    assert!(matches!(
        restore_snapshot(&s.path("snap"), &disk, &vars, true),
        Err(SnapshotError::VmRunning)
    ));
}

#[test]
fn an_interrupted_create_leaves_the_previous_snapshot_intact() {
    let s = Scratch::new("interrupted");
    let disk = s.write("disk", b"first");
    let vars = s.write("vars", b"first vars");
    let dest = s.path("snap");
    let first = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA).unwrap();

    // Simulate a crash partway through a second snapshot: a staging directory
    // exists with a half-written disk and no manifest.
    let staging = staging_path(&dest);
    fs::create_dir_all(&staging).unwrap();
    fs::write(staging.join(DISK_NAME), b"partial").unwrap();

    // The published snapshot is untouched and still verifies.
    assert_eq!(verify_snapshot(&dest).unwrap(), first);

    // And the next create clears the debris rather than mixing with it.
    fs::write(&disk, b"second").unwrap();
    let second = create_snapshot(&disk, &vars, &dest, "vm", false, QUOTA).unwrap();
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
    let m = create_snapshot(&disk, &vars, &s.path("snap"), "vm", false, u64::MAX).unwrap();

    assert_eq!(m.disk_bytes, bytes.len() as u64);
    assert_eq!(fs::read(s.path("snap").join(DISK_NAME)).unwrap(), bytes);
    verify_snapshot(&s.path("snap")).expect("a multi-chunk copy must verify");
}

#[path = "snapshot_manifest_json_tests.rs"]
mod manifest_json_tests;
