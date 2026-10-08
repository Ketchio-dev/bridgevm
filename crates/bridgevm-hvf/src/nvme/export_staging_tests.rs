use super::*;
use std::io::Write;

#[test]
fn allocated_names_are_exclusive_and_replaced_paths_are_not_removed() {
    let root = std::env::temp_dir().join(format!("bv-export-owner-{}", std::process::id()));
    fs::create_dir(&root).unwrap();
    let destination = root.join("output.raw");
    let mut first = ExportStaging::create(&root, &destination).unwrap();
    first.file.write_all(b"first-owned").unwrap();
    let second = ExportStaging::create(&root, &destination).unwrap();
    assert_ne!(first.path, second.path);
    assert_eq!(fs::metadata(&first.path).unwrap().mode() & 0o777, 0o600);
    assert_eq!(fs::read(&first.path).unwrap(), b"first-owned");
    let moved = root.join("retained-inode");
    fs::rename(&first.path, &moved).unwrap();
    fs::write(&first.path, b"replacement-not-owned").unwrap();
    let replaced = first.path.clone();
    let second_path = second.path.clone();
    drop(first);
    drop(second);
    assert_eq!(fs::read(replaced).unwrap(), b"replacement-not-owned");
    assert_eq!(fs::read(moved).unwrap(), b"first-owned");
    assert!(!second_path.exists());
    fs::remove_dir_all(root).unwrap();
}
