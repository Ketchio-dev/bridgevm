//! Each synthetic raw-file pair belongs to an exclusively created directory.

use super::super::LBA_SIZE;
use super::helpers::temp_path;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

pub(super) struct OwnedFiles {
    directory: PathBuf,
    pub paths: [PathBuf; 2],
}

impl OwnedFiles {
    pub fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory = temp_path(&format!("write-sync-{sequence}"));
        fs::create_dir(&directory).unwrap();
        let paths = [
            directory.join("primary.raw"),
            directory.join("secondary.raw"),
        ];
        for path in &paths {
            fs::write(path, vec![0u8; LBA_SIZE * 8]).unwrap();
        }
        Self { directory, paths }
    }
}

impl Drop for OwnedFiles {
    fn drop(&mut self) {
        for path in &self.paths {
            fs::remove_file(path).unwrap();
        }
        fs::remove_dir(&self.directory).unwrap();
    }
}
