//! Open and authenticate a cooperative lock before nonblocking acquisition.
use super::*;

pub(super) fn lock_key(root: &Path, key: &str, uid: u32) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .mode(0o600)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC | libc::O_NONBLOCK)
        .open(root.join(key))?;
    let metadata = file.metadata()?;
    if !metadata.is_file()
        || metadata.uid() != uid
        || metadata.nlink() != 1
        || metadata.permissions().mode() & 0o077 != 0
    {
        return Err(io::Error::other("unsafe media lease file"));
    }
    os::lock(&file, libc::LOCK_EX | libc::LOCK_NB)?;
    Ok(file)
}
