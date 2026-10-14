//! Unsafe source links never reach full-clone backing graph traversal.

use super::clone_fixture::source;
use crate::StorageError;
use std::fs;
use std::os::unix::fs::symlink;

#[test]
fn full_clone_refuses_source_file_and_directory_links_without_following_them() {
    for directory in [false, true] {
        let store = source("disks/root.raw", 0);
        let source = store.bundle_path("source");
        let outside = store.root().join("outside");
        let payload = if directory {
            fs::create_dir(&outside).unwrap();
            outside.join("backing.raw")
        } else {
            outside.clone()
        };
        fs::write(&payload, b"owned external target").unwrap();
        let link = source.join("disks/aliased.qcow2");
        symlink(&outside, &link).unwrap();
        let error = store.clone_vm("source", "copy", false).unwrap_err();
        assert!(matches!(error, StorageError::UnsupportedBundleEntry(path) if path == link));
        assert!(!store.bundle_path("copy").exists());
        assert_eq!(fs::read_link(&link).unwrap(), outside);
        assert_eq!(fs::read(payload).unwrap(), b"owned external target");
        assert_eq!(
            fs::read(source.join("disks/root.raw")).unwrap(),
            vec![0x6du8; 4096]
        );
        fs::remove_dir_all(store.root()).unwrap();
    }
}
