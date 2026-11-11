//! The converted raw image: written under a temporary name beside `dst` and
//! linked into place only once complete and durable, so a failed or
//! interrupted conversion leaves nothing at `dst`.

use std::fs::{self, File, OpenOptions};
use std::io;
use std::os::unix::fs::{FileExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

pub(super) struct RawOutput {
    file: File,
    temp: PathBuf,
    dst: PathBuf,
    size: u64,
}

impl RawOutput {
    /// Create the temporary file exclusively (O_EXCL, O_NOFOLLOW, 0600) and
    /// give it the virtual size; unwritten ranges stay holes.
    pub(super) fn create(dst: &Path, size: u64) -> io::Result<Self> {
        let name = dst.file_name().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "the destination needs a file name",
            )
        })?;
        if fs::symlink_metadata(dst).is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                format!("{} already exists", dst.display()),
            ));
        }
        let mut temp_name = std::ffi::OsString::from(".");
        temp_name.push(name);
        temp_name.push(format!(".converting-{}", std::process::id()));
        let temp = dst.with_file_name(temp_name);
        let file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&temp)?;
        let out = RawOutput {
            file,
            temp,
            dst: dst.to_path_buf(),
            size,
        };
        out.file.set_len(size)?;
        Ok(out)
    }

    pub(super) fn write_at(&self, bytes: &[u8], offset: u64) -> io::Result<()> {
        // Runs tile [0, size): a write past the end is a bug in a reader.
        debug_assert!(offset + bytes.len() as u64 <= self.size);
        self.file.write_all_at(bytes, offset)
    }

    /// Make the image durable, then publish it at `dst`. `link` refuses an
    /// existing `dst` (file or symlink), unlike `rename`; a volume without
    /// hard links (exFAT) gets a rename after the same check.
    pub(super) fn publish(self) -> io::Result<()> {
        // sync_all is F_FULLFSYNC on macOS.
        self.file.sync_all()?;
        match fs::hard_link(&self.temp, &self.dst) {
            Ok(()) => fs::remove_file(&self.temp)?,
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => return Err(error),
            Err(_) if fs::symlink_metadata(&self.dst).is_err() => {
                fs::rename(&self.temp, &self.dst)?
            }
            Err(error) => return Err(error),
        }
        let parent = self.dst.parent().filter(|p| !p.as_os_str().is_empty());
        File::open(parent.unwrap_or(Path::new(".")))?.sync_all()
    }
}

impl Drop for RawOutput {
    fn drop(&mut self) {
        // After publish the temporary name is already gone.
        let _ = fs::remove_file(&self.temp);
    }
}
