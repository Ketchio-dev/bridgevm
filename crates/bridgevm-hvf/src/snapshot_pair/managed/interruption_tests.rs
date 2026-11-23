//! A restore whose process exits at a fixed boundary, run in a child test
//! process: a fresh owner then finds one complete pair and can retry.

use super::*;
use crate::snapshot_pair::create_snapshot;
use crate::snapshot_pair::tests::Scratch;
use std::process::Command;

mod init_crash {
    use super::*;

    const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_INIT_CRASH";
    const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_INIT_DISK";
    const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_INIT_VARS";

    fn required_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("missing subprocess path {name}"))
    }

    fn selected_scenario(stage: InitStage) -> (&'static str, i32) {
        match stage {
            InitStage::DirectoryCreated => ("directory-created", 95),
            InitStage::MarkerSynced => ("marker-synced", 96),
            InitStage::RootPublished => ("root-published", 97),
        }
    }

    /// Subprocess entry point. A normal test run leaves this inert; the parent
    /// selects it alone and names the initialization boundary to die at.
    #[test]
    fn initialize_crash_child() {
        let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
            return;
        };
        let disk = required_path(DISK_ENV);
        let vars = required_path(VARS_ENV);
        let pair = LockedPair::open(&disk, &vars).expect("child owns pair");
        initialize(&pair.root, |stage| {
            let (name, exit) = selected_scenario(stage);
            if scenario == name {
                std::process::exit(exit);
            }
        })
        .expect("the selected initialization boundary must exit before return");
        panic!("unknown initialization crash scenario {scenario}");
    }

    fn crash_initialize(scenario: &str, exit: i32, disk: &Path, vars: &Path) {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("snapshot_pair::managed::interruption_tests::init_crash::initialize_crash_child")
            .arg("--exact")
            .arg("--nocapture")
            .env(SCENARIO_ENV, scenario)
            .env(DISK_ENV, disk)
            .env(VARS_ENV, vars)
            .status()
            .expect("launch initialization crash subprocess");
        assert_eq!(status.code(), Some(exit), "child status {status}");
    }

    fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
        let (disk, vars) = pair.paths().expect("resolve selected pair");
        (
            fs::read(disk).expect("read disk"),
            fs::read(vars).expect("read vars"),
        )
    }

    fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let source_disk = scratch.write("source-disk", b"new-disk");
        let source_vars = scratch.write("source-vars", b"new-vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&source_disk, &source_vars, &snapshot, "interrupted-init")
            .expect("create source snapshot");
        (scratch, disk, vars, snapshot)
    }

    fn managed_entries(scratch: &Scratch) -> Vec<std::ffi::OsString> {
        fs::read_dir(&scratch.0)
            .expect("list pair directory")
            .map(|entry| entry.expect("pair directory entry").file_name())
            .filter(|name| name.to_string_lossy().starts_with(".bridgevm-"))
            .collect()
    }

    /// No generation exists before initialization finishes, so a crash inside it
    /// must leave the originals selectable and a retried restore able to publish.
    #[test]
    fn hard_exit_during_store_initialization_keeps_pair_usable() {
        for stage in [
            InitStage::DirectoryCreated,
            InitStage::MarkerSynced,
            InitStage::RootPublished,
        ] {
            let (scenario, exit) = selected_scenario(stage);
            let (scratch, disk, vars, snapshot) = fixture(&format!("init-crash-{scenario}"));
            crash_initialize(scenario, exit, &disk, &vars);
            let mut pair = LockedPair::open(&disk, &vars)
                .unwrap_or_else(|error| panic!("{scenario} left the pair refused: {error}"));
            assert_eq!(
                contents(&pair),
                (b"old-disk".to_vec(), b"old-vars".to_vec()),
                "no generation was published before {scenario}"
            );
            pair.restore(&snapshot)
                .unwrap_or_else(|error| panic!("retry restore after {scenario}: {error}"));
            assert_eq!(
                contents(&pair),
                (b"new-disk".to_vec(), b"new-vars".to_vec()),
                "retry must publish one complete pair after {scenario}"
            );
            drop(pair);
            let reopened = LockedPair::open(&disk, &vars).expect("reopen restored pair");
            assert_eq!(
                contents(&reopened),
                (b"new-disk".to_vec(), b"new-vars".to_vec())
            );
            assert_eq!(
                managed_entries(&scratch),
                [reopened.root.file_name().unwrap().to_os_string()],
                "only the published root may remain after {scenario}"
            );
        }
    }
    /// An older binary creates the root and its marker in place, so a root can
    /// appear beside debris that a newer binary's interrupted setup left.
    fn older_binary_setup(root: &Path) {
        private_directory(root, true).expect("older binary creates root");
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(root.join("original"))
            .expect("older binary creates marker");
    }

    fn sorted_entries(scratch: &Scratch) -> Vec<std::ffi::OsString> {
        let mut entries = managed_entries(scratch);
        entries.sort();
        entries
    }

    /// Restore clears setup debris under the pair lease whether or not a root
    /// already exists.
    #[test]
    fn setup_debris_beside_an_existing_root_is_cleared() {
        for stage in [InitStage::DirectoryCreated, InitStage::MarkerSynced] {
            let (scenario, exit) = selected_scenario(stage);
            let (scratch, disk, vars, snapshot) = fixture(&format!("init-debris-{scenario}"));
            crash_initialize(scenario, exit, &disk, &vars);
            let mut pair = LockedPair::open(&disk, &vars).expect("open pair beside setup debris");
            older_binary_setup(&pair.root);
            assert_eq!(managed_entries(&scratch).len(), 2, "root and debris");
            pair.restore(&snapshot)
                .unwrap_or_else(|error| panic!("restore beside debris from {scenario}: {error}"));
            assert_eq!(
                contents(&pair),
                (b"new-disk".to_vec(), b"new-vars".to_vec())
            );
            assert_eq!(
                managed_entries(&scratch),
                [pair.root.file_name().unwrap().to_os_string()],
                "debris from {scenario} must be cleared"
            );
        }
    }

    /// Setup debris is removed only while it holds what setup writes. Anything
    /// else there is kept, and restore refuses with an error naming the entry.
    #[test]
    fn foreign_content_in_setup_debris_is_kept() {
        for root_exists in [false, true] {
            let (scratch, disk, vars, snapshot) = fixture(&format!("init-foreign-{root_exists}"));
            crash_initialize("directory-created", 95, &disk, &vars);
            let [debris] = managed_entries(&scratch)
                .try_into()
                .expect("one setup entry");
            let foreign = scratch.0.join(&debris).join("foreign");
            fs::write(&foreign, b"keep").expect("plant foreign file");
            let mut pair = LockedPair::open(&disk, &vars)
                .unwrap_or_else(|error| panic!("setup debris left the pair refused: {error}"));
            if root_exists {
                older_binary_setup(&pair.root);
            }
            let entries = sorted_entries(&scratch);
            let error = match pair.restore(&snapshot) {
                Err(SnapshotError::Io(error)) => error,
                other => panic!("root exists {root_exists}: foreign debris restore gave {other:?}"),
            };
            assert_eq!(error.kind(), io::ErrorKind::DirectoryNotEmpty, "{error}");
            assert!(
                error.to_string().contains(&*debris.to_string_lossy()),
                "error must name the kept entry: {error}"
            );
            assert_eq!(fs::read(&foreign).expect("foreign file kept"), b"keep");
            assert_eq!(sorted_entries(&scratch), entries, "managed entries changed");
            assert_eq!(
                contents(&pair),
                (b"old-disk".to_vec(), b"old-vars".to_vec())
            );
        }
    }
}

mod publish_crash {
    use super::staging_crash::crash_incomplete_restore;
    use super::*;
    use crate::snapshot_pair::verify_snapshot;
    use std::os::unix::fs::symlink;

    const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_CRASH";
    const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_DISK";
    const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_VARS";
    const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_SNAPSHOT";
    const BEFORE_EXIT: i32 = 71;
    const AFTER_EXIT: i32 = 72;

    fn required_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("missing subprocess path {name}"))
    }

    /// Subprocess entry point. A normal test run leaves this inert; the parent
    /// below selects it alone and supplies one exact crash boundary.
    #[test]
    fn restore_crash_child() {
        let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
            return;
        };
        let disk = required_path(DISK_ENV);
        let vars = required_path(VARS_ENV);
        let snapshot = required_path(SNAPSHOT_ENV);
        let mut pair = LockedPair::open(&disk, &vars).expect("child owns pair");
        pair.restore_using(&snapshot, |staged, current| match scenario.as_str() {
            "before-publish" => std::process::exit(BEFORE_EXIT),
            "after-publish" => {
                crate::snapshot_pair::publish::publish(staged, current)
                    .expect("publish restored pair");
                std::process::exit(AFTER_EXIT)
            }
            other => panic!("unknown crash scenario {other}"),
        })
        .expect("the crash boundary must exit before restore returns");
    }

    fn crash_restore(
        scenario: &str,
        expected_exit: i32,
        disk: &Path,
        vars: &Path,
        snapshot: &Path,
    ) {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
            .arg("snapshot_pair::managed::interruption_tests::publish_crash::restore_crash_child")
            .arg("--exact")
            .arg("--nocapture")
            .env(SCENARIO_ENV, scenario)
            .env(DISK_ENV, disk)
            .env(VARS_ENV, vars)
            .env(SNAPSHOT_ENV, snapshot)
            .status()
            .expect("launch crash subprocess");
        assert_eq!(status.code(), Some(expected_exit), "child status {status}");
    }

    fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let source_disk = scratch.write("source-disk", b"new-disk");
        let source_vars = scratch.write("source-vars", b"new-vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&source_disk, &source_vars, &snapshot, "interrupted-restore")
            .expect("create source snapshot");
        (scratch, disk, vars, snapshot)
    }

    fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
        let (disk, vars) = pair.paths().expect("resolve selected pair");
        (
            fs::read(disk).expect("read disk"),
            fs::read(vars).expect("read vars"),
        )
    }

    #[test]
    fn hard_exit_around_publication_reopens_only_complete_generations() {
        let (_scratch, disk, vars, snapshot) = fixture("managed-crash-before");
        crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).expect("reopen before-publish pair");
        assert_eq!(
            contents(&pair),
            (b"old-disk".to_vec(), b"old-vars".to_vec())
        );
        pair.restore(&snapshot).unwrap();
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec())
        );
        drop(pair);

        let (_scratch, disk, vars, snapshot) = fixture("managed-crash-after");
        crash_restore("after-publish", AFTER_EXIT, &disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).expect("reopen after-publish pair");
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec())
        );
        pair.restore(&snapshot)
            .expect("restore remains usable after unacknowledged publication");
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec())
        );
    }
    // Incomplete crash copies must be reclaimed before copying on retry.

    fn constrained_crash_retry(selected: bool) {
        let tag = format!("managed-capacity-crash-{selected}");
        let (_scratch, disk, vars, snapshot) = fixture(&tag);
        if selected {
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            pair.restore(&snapshot).unwrap();
            let (disk, vars) = pair.paths().unwrap();
            fs::write(disk, b"guest-disk").unwrap();
            fs::write(vars, b"guest-vars").unwrap();
        }
        crash_incomplete_restore(&disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let old_paths = pair.paths().unwrap();
        let old = contents(&pair);
        let manifest = verify_snapshot(&snapshot).unwrap();
        let needed = manifest.disk_bytes + manifest.vars_bytes;
        let staging = pair.root.join("staging");
        let stale_bytes = || {
            ["disk.raw", "vars.fd"]
                .iter()
                .filter_map(|name| fs::metadata(staging.join(name)).ok())
                .map(|metadata| metadata.len())
                .sum::<u64>()
        };
        assert_eq!(stale_bytes(), needed, "child must leave the complete pair");
        assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
        let result = pair.restore_copying(
            &snapshot,
            |src, dst| {
                if dst.file_name().unwrap() == "disk.raw" {
                    assert_eq!(stale_bytes(), 0, "stale copy not reclaimed before copy");
                    assert_eq!(pair_paths(&old_paths), old, "reclaim changed selection");
                }
                copy_and_sync(src, dst)
            },
            crate::snapshot_pair::publish::publish,
        );
        eprintln!("selected={selected} retry={result:?}");
        result.expect("reclaiming owned crash debris makes this retry fit exactly");
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec())
        );
        assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
        assert!(!staging.exists());
    }

    fn pair_paths(paths: &(PathBuf, PathBuf)) -> (Vec<u8>, Vec<u8>) {
        (fs::read(&paths.0).unwrap(), fs::read(&paths.1).unwrap())
    }

    #[test]
    fn initial_selection_retry_reclaims_crashed_staging_before_copy() {
        constrained_crash_retry(false);
    }

    #[test]
    fn current_selection_retry_reclaims_crashed_staging_before_copy() {
        constrained_crash_retry(true);
    }
    // Reclaiming stale copies must preserve every existing refusal boundary.

    fn refuse_copy(_: &Path, _: &Path) -> io::Result<u64> {
        panic!("refused admission reached copying")
    }

    fn refuse_publish(_: &Path, _: &Path) -> io::Result<()> {
        panic!("refused admission reached publication")
    }

    #[test]
    fn invalid_or_inside_managed_source_preserves_crash_debris() {
        for inside in [false, true] {
            let tag = format!("managed-retry-source-{inside}");
            let (_scratch, disk, vars, snapshot) = fixture(&tag);
            crash_restore("before-publish", BEFORE_EXIT, &disk, &vars, &snapshot);
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            let staged = pair.root.join("staging");
            let identity = fs::metadata(&staged).unwrap().ino();
            let source = if inside {
                staged.clone()
            } else {
                fs::write(snapshot.join("disk.raw"), b"bad-disk").unwrap();
                snapshot
            };
            assert!(pair
                .restore_copying(&source, refuse_copy, refuse_publish)
                .is_err());
            assert_eq!(fs::metadata(&staged).unwrap().ino(), identity);
            assert_eq!(fs::read(staged.join("disk.raw")).unwrap(), b"new-disk");
            assert_eq!(
                contents(&pair),
                (b"old-disk".to_vec(), b"old-vars".to_vec())
            );
        }
    }

    #[test]
    fn replaced_root_symlink_refuses_before_cleanup_or_copy() {
        let (scratch, disk, vars, snapshot) = fixture("managed-retry-root-link");
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        initialize(&pair.root, |_| {}).unwrap();
        let moved = scratch.path("original-root");
        fs::rename(&pair.root, &moved).unwrap();
        let unrelated = scratch.path("unrelated");
        private_directory(&unrelated, true).unwrap();
        let staged = unrelated.join("staging");
        private_directory(&staged, true).unwrap();
        fs::write(staged.join("keep"), b"unrelated bytes").unwrap();
        let inode = fs::metadata(staged.join("keep")).unwrap().ino();
        symlink(&unrelated, &pair.root).unwrap();
        assert!(pair
            .restore_copying(&snapshot, refuse_copy, refuse_publish)
            .is_err());
        assert!(fs::symlink_metadata(&pair.root)
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::metadata(staged.join("keep")).unwrap().ino(), inode);
        assert_eq!(fs::read(staged.join("keep")).unwrap(), b"unrelated bytes");
        assert!(moved.join("original").is_file());
        assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
        assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
    }

    #[test]
    fn unowned_staging_refuses_before_copy_and_preserves_selection() {
        for selected in [false, true] {
            for kind in ["symlink", "file", "permissive"] {
                let tag = format!("managed-retry-stage-{selected}-{kind}");
                let (scratch, disk, vars, snapshot) = fixture(&tag);
                let mut pair = LockedPair::open(&disk, &vars).unwrap();
                if selected {
                    pair.restore(&snapshot).unwrap();
                } else {
                    initialize(&pair.root, |_| {}).unwrap();
                }
                let old_paths = pair.paths().unwrap();
                let old = contents(&pair);
                let unrelated = scratch.path("unrelated");
                private_directory(&unrelated, true).unwrap();
                fs::write(unrelated.join("keep"), b"unrelated bytes").unwrap();
                let staged = pair.root.join("staging");
                match kind {
                    "symlink" => symlink(&unrelated, &staged).unwrap(),
                    "file" => fs::write(&staged, b"keep file").unwrap(),
                    _ => {
                        fs::create_dir(&staged).unwrap();
                        fs::set_permissions(&staged, fs::Permissions::from_mode(0o755)).unwrap();
                        fs::write(staged.join("keep"), b"keep directory").unwrap();
                    }
                }
                let metadata = fs::symlink_metadata(&staged).unwrap();
                assert!(pair
                    .restore_copying(&snapshot, refuse_copy, refuse_publish)
                    .is_err());
                let after = fs::symlink_metadata(&staged).unwrap();
                assert_eq!(
                    (after.dev(), after.ino(), after.mode()),
                    (metadata.dev(), metadata.ino(), metadata.mode())
                );
                assert_eq!(
                    fs::read(unrelated.join("keep")).unwrap(),
                    b"unrelated bytes"
                );
                if kind == "file" {
                    assert_eq!(fs::read(&staged).unwrap(), b"keep file");
                } else if kind == "permissive" {
                    assert_eq!(fs::read(staged.join("keep")).unwrap(), b"keep directory");
                }
                assert_eq!(pair.paths().unwrap(), old_paths);
                assert_eq!(contents(&pair), old);
                assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
                assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
            }
        }
    }

    #[test]
    fn copy_enospc_after_reclaim_preserves_original_or_current_pair() {
        for selected in [false, true] {
            let tag = format!("managed-retry-still-full-{selected}");
            let (_scratch, disk, vars, snapshot) = fixture(&tag);
            if selected {
                LockedPair::open(&disk, &vars)
                    .unwrap()
                    .restore(&snapshot)
                    .unwrap();
            }
            crash_incomplete_restore(&disk, &vars, &snapshot);
            let mut pair = LockedPair::open(&disk, &vars).unwrap();
            let paths = pair.paths().unwrap();
            let old = contents(&pair);
            let staged = pair.root.join("staging");
            let result = pair.restore_copying(
                &snapshot,
                |_, _| {
                    assert!(
                        !staged.join("disk.raw").exists(),
                        "stale disk not reclaimed before copy"
                    );
                    Err(io::Error::from_raw_os_error(libc::ENOSPC))
                },
                refuse_publish,
            );
            assert!(
                matches!(result, Err(SnapshotError::Io(e)) if e.raw_os_error() == Some(libc::ENOSPC))
            );
            assert_eq!(pair.paths().unwrap(), paths);
            assert_eq!(contents(&pair), old);
            assert_eq!(fs::read(&disk).unwrap(), b"old-disk");
            assert_eq!(fs::read(&vars).unwrap(), b"old-vars");
            assert!(!staged.exists());
        }
    }

    #[test]
    fn initial_copy_enospc_preserves_original_and_clears_staging() {
        let (_scratch, disk, vars, snapshot) = fixture("managed-retry-new-full");
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let root = pair.root.clone();
        assert!(!root.exists());
        let result = pair.restore_copying(
            &snapshot,
            |_, _| Err(io::Error::from_raw_os_error(libc::ENOSPC)),
            refuse_publish,
        );
        assert!(
            matches!(result, Err(SnapshotError::Io(e)) if e.raw_os_error() == Some(libc::ENOSPC))
        );
        assert!(!root.join("staging").exists());
        assert_eq!(
            contents(&pair),
            (b"old-disk".to_vec(), b"old-vars".to_vec())
        );
    }

    #[test]
    fn retry_copies_after_reclaim() {
        let (_scratch, disk, vars, snapshot) = fixture("managed-retry-unknown-space");
        crash_incomplete_restore(&disk, &vars, &snapshot);
        let mut pair = LockedPair::open(&disk, &vars).unwrap();
        let staged = pair.root.join("staging");
        pair.restore_copying(
            &snapshot,
            |src, dst| {
                if dst.file_name().unwrap() == "disk.raw" {
                    assert!(!staged.join("disk.raw").exists());
                }
                crate::snapshot_pair::files::copy_and_sync(src, dst)
            },
            crate::snapshot_pair::publish::publish,
        )
        .unwrap();
        assert_eq!(
            contents(&pair),
            (b"new-disk".to_vec(), b"new-vars".to_vec())
        );
    }
}

mod staging_crash {
    use super::*;

    const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_CRASH";
    const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_DISK";
    const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_VARS";
    const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STAGING_SNAPSHOT";

    fn required_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("missing subprocess path {name}"))
    }

    #[test]
    fn restore_partial_staging_crash_child() {
        let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
            return;
        };
        let disk = required_path(DISK_ENV);
        let vars = required_path(VARS_ENV);
        let snapshot = required_path(SNAPSHOT_ENV);
        let pair = LockedPair::open(&disk, &vars).expect("child owns pair");
        initialize(&pair.root, |_| {}).expect("initialize managed root");
        let staged = pair.root.join("staging");
        private_directory(&staged, true).expect("create private staging directory");
        copy_and_sync(&snapshot.join("disk.raw"), &staged.join("disk.raw")).expect("stage disk");
        if scenario == "disk-only" {
            std::process::exit(73);
        }
        copy_and_sync(&snapshot.join("vars.fd"), &staged.join("vars.fd")).expect("stage vars");
        if scenario == "disk-and-vars" {
            std::process::exit(74);
        }
        copy_and_sync(
            &snapshot.join("manifest.json"),
            &staged.join("manifest.json"),
        )
        .expect("stage manifest");
        if scenario == "complete-before-directory-sync" {
            std::process::exit(75);
        }
        panic!("unknown crash scenario {scenario}");
    }

    fn fixture(tag: &str) -> (Scratch, PathBuf, PathBuf, PathBuf) {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let source_disk = scratch.write("source-disk", b"new-disk");
        let source_vars = scratch.write("source-vars", b"new-vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(
            &source_disk,
            &source_vars,
            &snapshot,
            "partial-staging-restore",
        )
        .expect("create source snapshot");
        (scratch, disk, vars, snapshot)
    }

    fn contents(pair: &LockedPair) -> (Vec<u8>, Vec<u8>) {
        let (disk, vars) = pair.paths().expect("resolve selected pair");
        (
            fs::read(disk).expect("read disk"),
            fs::read(vars).expect("read vars"),
        )
    }

    #[test]
    fn hard_exit_during_partial_staging_preserves_old_pair_and_allows_retry() {
        for (scenario, exit) in [
            ("disk-only", 73),
            ("disk-and-vars", 74),
            ("complete-before-directory-sync", 75),
        ] {
            let (_scratch, disk, vars, snapshot) = fixture(scenario);
            crash_restore(scenario, exit, &disk, &vars, &snapshot);
            let mut pair = LockedPair::open(&disk, &vars).expect("reopen partially staged pair");
            assert_eq!(
                contents(&pair),
                (b"old-disk".to_vec(), b"old-vars".to_vec()),
                "partial staging must never become selected for {scenario}"
            );
            retry_after_explicit_staging_resolution(&mut pair, &snapshot);
            assert_eq!(
                contents(&pair),
                (b"new-disk".to_vec(), b"new-vars".to_vec()),
                "retry must publish one complete pair for {scenario}"
            );
        }
    }
    // Tests model the user's explicit resolution of ambiguous complete staging.

    fn retry_after_explicit_staging_resolution(pair: &mut LockedPair, snapshot: &Path) {
        let staged = pair.root.join("staging");
        if staged.join("manifest.json").exists() {
            let read = |root: &Path| {
                ["disk.raw", "vars.fd", "manifest.json"]
                    .map(|name| fs::read(root.join(name)).unwrap())
            };
            let before = read(&staged);
            let selected = pair.paths().unwrap();
            let refusal = pair
                .restore_copying(
                    snapshot,
                    |_, _| panic!("ambiguous staging must refuse before copy"),
                    |_, _| panic!("ambiguous staging must refuse before publication"),
                )
                .unwrap_err();
            assert!(refusal.to_string().contains("it was left intact"));
            assert_eq!(read(&staged), before);
            assert_eq!(pair.paths().unwrap(), selected);
            let preserved = pair
                .disk
                .parent()
                .unwrap()
                .join("preserved-complete-staging");
            fs::rename(&staged, &preserved)
                .expect("simulate explicit user move outside managed storage");
            assert_eq!(read(&preserved), before);
        }
        pair.restore(snapshot)
            .expect("retry after explicit staging resolution");
    }

    pub(super) fn crash_incomplete_restore(disk: &Path, vars: &Path, snapshot: &Path) {
        crash_restore("disk-and-vars", 74, disk, vars, snapshot);
    }

    fn crash_restore(
        scenario: &str,
        expected_exit: i32,
        disk: &Path,
        vars: &Path,
        snapshot: &Path,
    ) {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg(
            "snapshot_pair::managed::interruption_tests::staging_crash::restore_partial_staging_crash_child",
        )
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, scenario)
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .env(SNAPSHOT_ENV, snapshot)
        .status()
        .expect("launch crash subprocess");
        assert_eq!(status.code(), Some(expected_exit), "child status {status}");
    }
}

mod streaming_crash {
    use super::*;
    use crate::snapshot_pair::files::{stream_and_sync, COPY_CHUNK};
    use libc::ENOSPC;

    const SCENARIO_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_CRASH";
    const DISK_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_DISK";
    const VARS_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_VARS";
    const SNAPSHOT_ENV: &str = "BRIDGEVM_TEST_MANAGED_PAIR_STREAM_SNAPSHOT";
    const FIRST_CHUNK_EXIT: i32 = 76;

    type Pair = (Vec<u8>, Vec<u8>);

    fn required_path(name: &str) -> PathBuf {
        std::env::var_os(name)
            .map(PathBuf::from)
            .unwrap_or_else(|| panic!("missing subprocess path {name}"))
    }

    fn refuse_publication(_: &Path, _: &Path) -> io::Result<()> {
        panic!("a partially staged restore reached publication")
    }

    /// Subprocess entry point. The restore streams, as it does when cloning is
    /// refused, and the process exits once the first disk chunk is written.
    #[test]
    fn restore_streaming_crash_child() {
        let Ok(scenario) = std::env::var(SCENARIO_ENV) else {
            return;
        };
        assert_eq!(scenario, "first-chunk", "unknown crash scenario");
        let disk = required_path(DISK_ENV);
        let vars = required_path(VARS_ENV);
        let snapshot = required_path(SNAPSHOT_ENV);
        let mut pair = LockedPair::open(&disk, &vars).expect("child owns pair");
        let exit_after_first_chunk = |src: &Path, dst: &Path| {
            stream_and_sync(src, dst, |_| std::process::exit(FIRST_CHUNK_EXIT))
        };
        pair.restore_copying(&snapshot, exit_after_first_chunk, refuse_publication)
            .expect("the crash boundary must exit before restore returns");
    }

    fn crash_restore(disk: &Path, vars: &Path, snapshot: &Path) {
        let status = Command::new(std::env::current_exe().expect("current test executable"))
        .arg("snapshot_pair::managed::interruption_tests::streaming_crash::restore_streaming_crash_child")
        .arg("--exact")
        .arg("--nocapture")
        .env(SCENARIO_ENV, "first-chunk")
        .env(DISK_ENV, disk)
        .env(VARS_ENV, vars)
        .env(SNAPSHOT_ENV, snapshot)
        .status()
        .expect("launch crash subprocess");
        assert_eq!(
            status.code(),
            Some(FIRST_CHUNK_EXIT),
            "child status {status}"
        );
    }

    /// The snapshot's disk spans two chunks, so one chunk is a partial copy.
    /// With `selected`, an earlier restore has made a managed generation current.
    fn fixture(tag: &str, selected: bool) -> (Scratch, PathBuf, PathBuf, PathBuf, Pair, Pair) {
        let scratch = Scratch::new(tag);
        let disk = scratch.write("disk", b"old-disk");
        let vars = scratch.write("vars", b"old-vars");
        let new_disk = vec![7; COPY_CHUNK + 4096];
        let source_disk = scratch.write("source-disk", &new_disk);
        let source_vars = scratch.write("source-vars", b"new-vars");
        let snapshot = scratch.path("snapshot");
        create_snapshot(&source_disk, &source_vars, &snapshot, "stream")
            .expect("create source snapshot");
        let mut old = (b"old-disk".to_vec(), b"old-vars".to_vec());
        if selected {
            let first = scratch.path("first");
            create_snapshot(&disk, &vars, &first, "first").expect("first snapshot");
            let mut pair = LockedPair::open(&disk, &vars).expect("own pair");
            pair.restore(&first).expect("select a managed generation");
            let (selected_disk, selected_vars) = pair.paths().expect("selected pair");
            fs::write(selected_disk, b"guest-disk").expect("guest writes disk");
            fs::write(selected_vars, b"guest-vars").expect("guest writes vars");
            old = (b"guest-disk".to_vec(), b"guest-vars".to_vec());
        }
        let new = (new_disk, b"new-vars".to_vec());
        (scratch, disk, vars, snapshot, old, new)
    }

    fn contents(pair: &LockedPair) -> Pair {
        let (disk, vars) = pair.paths().expect("resolve selected pair");
        (
            fs::read(disk).expect("read disk"),
            fs::read(vars).expect("read vars"),
        )
    }

    #[test]
    fn hard_exit_mid_streaming_restore_keeps_selected_pair_and_allows_retry() {
        for selected in [false, true] {
            let tag = format!("managed-stream-crash-{selected}");
            let (_scratch, disk, vars, snapshot, old, new) = fixture(&tag, selected);
            crash_restore(&disk, &vars, &snapshot);
            let mut pair = LockedPair::open(&disk, &vars).expect("reopen after crash");
            let staged = fs::metadata(pair.root.join("staging/disk.raw")).map(|m| m.len());
            assert_eq!(
                staged.ok(),
                Some(COPY_CHUNK as u64),
                "child did not stop mid-disk"
            );
            assert_eq!(contents(&pair), old, "partial staging became selected");
            pair.restore(&snapshot).expect("retry restore after crash");
            assert_eq!(contents(&pair), new, "retry did not select the snapshot");
        }
    }
    /// A full volume ends the streaming copy after one chunk with an error, not
    /// a crash, so restore itself must remove the partial copy.
    #[test]
    fn a_full_volume_mid_streaming_restore_keeps_selected_pair_and_no_partial_copy() {
        for selected in [false, true] {
            let tag = format!("managed-stream-full-{selected}");
            let (_scratch, disk, vars, snapshot, old, new) = fixture(&tag, selected);
            let mut pair = LockedPair::open(&disk, &vars).expect("own pair");
            let full = |src: &Path, dst: &Path| {
                stream_and_sync(src, dst, |_| Err(io::Error::from_raw_os_error(ENOSPC)))
            };
            match pair.restore_copying(&snapshot, full, refuse_publication) {
                Err(SnapshotError::Io(error)) => assert_eq!(error.raw_os_error(), Some(ENOSPC)),
                other => panic!("expected the full-volume error, got {other:?}"),
            }
            assert_eq!(contents(&pair), old, "failed restore changed the selection");
            let staging = pair.root.join("staging");
            assert!(
                !staging.exists(),
                "failed restore stranded its partial copy"
            );
            let stream = |src: &Path, dst: &Path| stream_and_sync(src, dst, |_| Ok(()));
            let publish = crate::snapshot_pair::publish::publish;
            pair.restore_copying(&snapshot, stream, publish)
                .expect("streaming retry after a full volume");
            assert_eq!(contents(&pair), new, "retry did not select the snapshot");
        }
    }
}
