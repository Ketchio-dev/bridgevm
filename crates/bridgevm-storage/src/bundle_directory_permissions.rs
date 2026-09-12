use std::fs;
use std::io;
#[cfg(unix)]
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::{Path, PathBuf};

pub(crate) fn create(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    builder.mode(0o700);
    builder.create(path)
}

pub(crate) fn copy_mode(source: &Path, destination: &Path) -> io::Result<()> {
    let permissions = fs::metadata(source)?.permissions();
    #[cfg(unix)]
    {
        set_mode(destination, permissions.mode())
    }
    #[cfg(not(unix))]
    {
        fs::set_permissions(destination, permissions)
    }
}

fn set_mode(path: &Path, mode: u32) -> io::Result<()> {
    #[cfg(unix)]
    {
        fs::set_permissions(path, fs::Permissions::from_mode(mode & 0o777))
    }
    #[cfg(not(unix))]
    {
        let _ = (path, mode);
        Ok(())
    }
}

#[derive(Default)]
pub(crate) struct DirectoryModes(Vec<(PathBuf, u32)>);

impl DirectoryModes {
    pub(crate) fn record(&mut self, path: PathBuf, mode: u32) -> io::Result<()> {
        create(&path)?;
        self.0.retain(|(existing, _)| existing != &path);
        self.0.push((path, mode));
        Ok(())
    }

    pub(crate) fn apply(mut self) -> io::Result<()> {
        self.0
            .sort_by_key(|(path, _)| std::cmp::Reverse(path.components().count()));
        for (path, mode) in self.0 {
            set_mode(&path, mode)?;
        }
        Ok(())
    }
}
