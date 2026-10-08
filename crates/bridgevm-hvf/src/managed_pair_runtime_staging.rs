//! Exclusively created staging; cleanup only removes its own surviving inode.

use std::fs::{File, OpenOptions};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::sync::atomic::{AtomicU64, Ordering};
use std::{
    fs, io,
    path::{Path, PathBuf},
};

pub(super) struct StagedFile {
    pub(super) path: PathBuf,
    pub(super) file: File,
}

impl StagedFile {
    pub(super) fn create(destination: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = destination.parent().unwrap();
        for _ in 0..128 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".bridgevm-write-{}-{sequence}.tmp",
                std::process::id()
            ));
            if path
                .file_name()
                .unwrap()
                .as_bytes()
                .eq_ignore_ascii_case(destination.file_name().unwrap().as_bytes())
            {
                continue;
            }
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free media staging name",
        ))
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        // Publication may have renamed the file even when it returned Err.
        // Clean only our own remaining inode, never a replacement at this name.
        if let (Ok(path), Ok(file)) = (fs::symlink_metadata(&self.path), self.file.metadata()) {
            if (path.dev(), path.ino()) == (file.dev(), file.ino()) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}
