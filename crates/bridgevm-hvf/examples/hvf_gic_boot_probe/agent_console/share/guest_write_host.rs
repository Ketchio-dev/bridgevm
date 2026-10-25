//! The host side of sealing a guest-written temporary: the type of the volume
//! holding the destination directory, the temporary's mode after chmod, and
//! its com.apple.quarantine mark. guest_write_policy.rs decides from these;
//! tests substitute them instead of mounting FAT or exFAT images.

use std::ffi::CStr;
use std::fs::File;
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;

const QUARANTINE: &CStr = c"com.apple.quarantine";

pub(super) trait SealHost {
    /// statfs f_fstypename of the volume holding the destination directory.
    fn filesystem(&mut self) -> io::Result<String>;
    /// The temporary's mode, read after any chmod.
    fn mode(&mut self, file: &File) -> io::Result<u32>;
    fn set_mark(&mut self, file: &File, mark: &[u8]) -> io::Result<()>;
    fn get_mark(&mut self, file: &File) -> io::Result<Vec<u8>>;
}

/// fstatfs, fstat and the extended attribute, through live descriptors.
pub(super) struct NativeSealHost<'a> {
    /// The destination directory the temporary was created in.
    pub(super) directory: &'a File,
}

impl SealHost for NativeSealHost<'_> {
    fn filesystem(&mut self) -> io::Result<String> {
        let mut stat = std::mem::MaybeUninit::<libc::statfs>::uninit();
        // SAFETY: live directory descriptor and writable statfs storage.
        if unsafe { libc::fstatfs(self.directory.as_raw_fd(), stat.as_mut_ptr()) } != 0 {
            return Err(io::Error::last_os_error());
        }
        // SAFETY: fstatfs succeeded, so it initialized the storage.
        let name = unsafe { stat.assume_init() }.f_fstypename.map(|c| c as u8);
        let name = CStr::from_bytes_until_nul(&name)
            .map_err(|_| io::Error::other("f_fstypename is not terminated"))?;
        Ok(name.to_string_lossy().into_owned())
    }

    fn mode(&mut self, file: &File) -> io::Result<u32> {
        Ok(file.metadata()?.permissions().mode())
    }

    fn set_mark(&mut self, file: &File, mark: &[u8]) -> io::Result<()> {
        let (fd, name) = (file.as_raw_fd(), QUARANTINE.as_ptr());
        // SAFETY: live descriptor, NUL-terminated name, `mark` is mark.len() bytes.
        let status = unsafe { libc::fsetxattr(fd, name, mark.as_ptr().cast(), mark.len(), 0, 0) };
        if status != 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn get_mark(&mut self, file: &File) -> io::Result<Vec<u8>> {
        // A BridgeVM mark is 59 bytes; a longer value is not one (ERANGE).
        let mut value = vec![0u8; 64];
        let (fd, name) = (file.as_raw_fd(), QUARANTINE.as_ptr());
        // SAFETY: live descriptor, NUL-terminated name, value.len() writable bytes.
        let len =
            unsafe { libc::fgetxattr(fd, name, value.as_mut_ptr().cast(), value.len(), 0, 0) };
        let Ok(len) = usize::try_from(len) else {
            return Err(io::Error::last_os_error());
        };
        value.truncate(len);
        Ok(value)
    }
}
