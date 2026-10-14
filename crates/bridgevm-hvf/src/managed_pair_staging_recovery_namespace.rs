//! Capture synthetic evidence without opening a symlink, FIFO, or other device.

use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
};

#[derive(Debug, Eq, PartialEq)]
pub(in super::super) enum Contents {
    Directory,
    File(Vec<u8>),
    Symlink(PathBuf),
    Special,
}

type Entry = (u64, u64, u32, Contents);

pub(in super::super) fn capture(root: &Path) -> BTreeMap<PathBuf, Entry> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<PathBuf, Entry>) {
        let metadata = fs::symlink_metadata(path).unwrap();
        let contents = if metadata.is_dir() {
            Contents::Directory
        } else if metadata.is_file() {
            Contents::File(fs::read(path).unwrap())
        } else if metadata.file_type().is_symlink() {
            Contents::Symlink(fs::read_link(path).unwrap())
        } else {
            Contents::Special
        };
        result.insert(
            path.strip_prefix(root).unwrap().to_path_buf(),
            (metadata.dev(), metadata.ino(), metadata.mode(), contents),
        );
        if metadata.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                visit(root, &entry.unwrap().path(), result);
            }
        }
    }
    let mut result = BTreeMap::new();
    visit(root, root, &mut result);
    result
}
