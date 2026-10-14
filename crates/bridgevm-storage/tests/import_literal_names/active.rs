use bridgevm_storage::VmStore;
use std::path::Path;

pub(super) fn assert_imported_disk(target: &VmStore, expected: &Path) {
    let active = target.active_disk("raw-copy").unwrap();
    assert_eq!(active.path, expected.canonicalize().unwrap());
    assert!(active.exists);
}
