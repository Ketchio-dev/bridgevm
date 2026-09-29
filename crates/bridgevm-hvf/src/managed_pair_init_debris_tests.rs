use super::*;
use std::os::unix::fs::OpenOptionsExt;

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
