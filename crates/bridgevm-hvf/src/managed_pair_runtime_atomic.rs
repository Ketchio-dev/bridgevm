//! Fresh staged files are owned before publication; uncertain writes stay owned.

use super::*;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::sync::atomic::{AtomicU64, Ordering};

impl RuntimeLease {
    pub(super) fn write_owned(
        &mut self,
        path: &Path,
        bytes: &[u8],
        publish: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("media output has no filename"))?;
        let destination = fs::canonicalize(parent)?.join(name);
        self.owner().extend([destination.as_path()])?;
        // Keep this path even if publication later returns an uncertain error.
        self.retained.insert(destination.clone());
        let mut staged = StagedFile::create(&destination)?;
        self.owner().extend([staged.path.as_path()])?;
        staged.file.write_all(bytes)?;
        staged.file.sync_all()?;
        publish(&staged.path, &destination)?;
        let retained: Vec<_> = self.retained.iter().cloned().collect();
        self.owner().replace(retained.iter().map(PathBuf::as_path))
    }
}

struct StagedFile {
    path: PathBuf,
    file: File,
}

impl StagedFile {
    fn create(destination: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = destination.parent().unwrap();
        for _ in 0..128 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".bridgevm-write-{}-{sequence}.tmp",
                std::process::id()
            ));
            if path == destination {
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
