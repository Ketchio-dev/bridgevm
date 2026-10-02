//! Resolve missing output tails too, while detecting existing hard-link aliases.
use std::os::unix::fs::MetadataExt;
use std::{
    fs, io,
    path::{Path, PathBuf},
};
fn resolved(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("media output has no filename"))?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            Ok(resolved(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}
pub(super) fn same(left: &Path, right: &Path) -> io::Result<bool> {
    let (left, right) = (resolved(left)?, resolved(right)?);
    if left == right || case_alias(&left, &right)? {
        return Ok(true);
    }
    let identity = |path: &Path| match fs::metadata(path) {
        Ok(metadata) => Ok(Some((metadata.dev(), metadata.ino()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    };
    Ok(matches!((identity(&left)?, identity(&right)?), (Some(left), Some(right)) if left == right))
}

#[cfg(target_os = "macos")]
fn case_alias(left: &Path, right: &Path) -> io::Result<bool> {
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    if left.parent() != right.parent()
        || !left
            .file_name()
            .unwrap()
            .as_bytes()
            .eq_ignore_ascii_case(right.file_name().unwrap().as_bytes())
    {
        return Ok(false);
    }
    let parent = fs::File::open(left.parent().unwrap())?;
    // SAFETY: the file owns this descriptor and fpathconf only queries it.
    let sensitive = unsafe { libc::fpathconf(parent.as_raw_fd(), libc::_PC_CASE_SENSITIVE) };
    if sensitive < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(sensitive == 0)
}
#[cfg(not(target_os = "macos"))]
fn case_alias(_: &Path, _: &Path) -> io::Result<bool> {
    Ok(false)
}
