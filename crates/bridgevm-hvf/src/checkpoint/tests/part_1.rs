//! Split test module.

use super::super::*;
use std::fs::{self};

use super::helpers::*;

#[test]
fn durable_write_replaces_checkpoint_without_leaving_temporary_files() {
    let directory = test_directory("atomic-replace");
    fs::create_dir_all(&directory).unwrap();
    let path = directory.join("suspend.bin");

    test_checkpoint(vec![1]).write_to_path(&path).unwrap();
    test_checkpoint(vec![9, 8, 7]).write_to_path(&path).unwrap();

    let restored = VmCheckpoint::read_from_path(&path).unwrap();
    assert_eq!(restored.device_state, vec![9, 8, 7]);
    assert_eq!(restored.ram_len, SPARSE_RAM_CHUNK_SIZE as u64);
    assert_eq!(restored.vcpus[0].pc, 0x4000_1000);
    assert!(fs::read_dir(&directory).unwrap().all(|entry| !entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .ends_with(".tmp")));

    fs::remove_dir_all(directory).unwrap();
}
