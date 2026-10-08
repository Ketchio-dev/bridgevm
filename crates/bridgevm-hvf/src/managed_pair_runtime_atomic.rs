//! Fresh staged files are owned before publication; uncertain writes stay owned.

use super::*;
use std::io::Write;
#[path = "managed_pair_runtime_staging.rs"]
mod staging;
use staging::StagedFile;

impl RuntimeLease {
    pub(super) fn write_owned(
        &mut self,
        path: &Path,
        bytes: &[u8],
        publish: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        self.write_owned_with(path, |out| out.write_all(bytes), publish)
    }

    pub(super) fn write_owned_with<T>(
        &mut self,
        path: &Path,
        write: impl FnOnce(&mut dyn Write) -> io::Result<T>,
        publish: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<T> {
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
        let result = write(&mut staged.file)?;
        staged.file.sync_all()?;
        publish(&staged.path, &destination)?;
        let retained: Vec<_> = self.retained.iter().cloned().collect();
        self.owner()
            .replace(retained.iter().map(PathBuf::as_path))?;
        Ok(result)
    }
}
