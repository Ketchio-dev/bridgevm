//! What a guest-written share file carries onto the Mac (decisions of
//! 2026-10-23). It never carries an execute bit: a new file is created 0644
//! before the umask, and an overwrite keeps only the replaced file's read/write
//! bits, so setuid, setgid and sticky stay cleared too. It always carries a
//! com.apple.quarantine mark, set on the unpublished temporary. A temporary
//! that still reports one of those bits (FAT and exFAT report every file as
//! rwx------) or whose mark cannot be set refuses the write, including on a
//! share filesystem without extended attributes (ENOTSUP).

use bridgevm_hvf::host_entropy::HostEntropy;
use bridgevm_hvf::smccc_trng::EntropySource;
use std::ffi::CStr;
use std::fs::{File, Permissions};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::PermissionsExt;
use std::time::{SystemTime, UNIX_EPOCH};

/// Mode a new guest file is created with, before the process umask.
pub(super) const NEW_FILE_MODE: libc::mode_t = 0o644;
const QUARANTINE: &CStr = c"com.apple.quarantine";

/// Make the unpublished temporary `file` publishable over `existing`, or fail.
pub(super) fn seal(existing: Option<&File>, file: &File) -> io::Result<()> {
    seal_with(existing, file, &mut HostEntropy::new(), set_xattr)
}

pub(super) fn seal_with(
    existing: Option<&File>,
    file: &File,
    entropy: &mut impl EntropySource,
    set: impl FnOnce(&File, &CStr, &[u8]) -> io::Result<()>,
) -> io::Result<()> {
    if let Some(existing) = existing {
        #[cfg(target_os = "macos")]
        {
            // SAFETY: both descriptors are live; no data/stat copy so the
            // replacement keeps its new content and modification time.
            let status = unsafe {
                libc::fcopyfile(
                    existing.as_raw_fd(),
                    file.as_raw_fd(),
                    std::ptr::null_mut(),
                    libc::COPYFILE_ACL | libc::COPYFILE_XATTR,
                )
            };
            if status != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        // After the copy, which also adds owner read/write to the temporary.
        let mode = existing.metadata()?.permissions().mode() & 0o666;
        file.set_permissions(Permissions::from_mode(mode))?;
    }
    // Verified, not assumed: some filesystems ignore the create mode and chmod.
    if file.metadata()?.permissions().mode() & 0o7111 != 0 {
        let reason = "execute bits refused: share filesystem keeps them on guest files";
        return Err(io::Error::new(io::ErrorKind::PermissionDenied, reason));
    }
    // Last: fcopyfile re-applies the replaced file's own quarantine, if any.
    let mark = quarantine_mark(SystemTime::now(), entropy)?;
    set(file, QUARANTINE, mark.as_bytes()).map_err(|error| {
        if error.raw_os_error() == Some(libc::ENOTSUP) {
            let reason = "quarantine refused: share filesystem has no extended attributes";
            return io::Error::new(io::ErrorKind::Unsupported, reason);
        }
        io::Error::new(error.kind(), format!("quarantine not set: {error}"))
    })
}

/// `flags;hex-seconds;agent;UUID`, the form of the system's own marks, with
/// the decided flags 0083 (Apple does not document the flag bits). The UUID is
/// version 4 from the host CSPRNG, with no fallback.
pub(super) fn quarantine_mark(
    now: SystemTime,
    entropy: &mut impl EntropySource,
) -> io::Result<String> {
    let mut id = [0u8; 16];
    entropy
        .fill(&mut id)
        .map_err(|_| io::Error::other("quarantine refused: host CSPRNG unavailable"))?;
    // Version 4 (random) and the RFC 9562 variant.
    id[6] = (id[6] & 0x0f) | 0x40;
    id[8] = (id[8] & 0x3f) | 0x80;
    let hex: String = id.iter().map(|byte| format!("{byte:02X}")).collect();
    let seconds = now.duration_since(UNIX_EPOCH).map_or(0, |at| at.as_secs());
    Ok(format!(
        "0083;{seconds:08x};BridgeVM;{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

fn set_xattr(file: &File, name: &CStr, value: &[u8]) -> io::Result<()> {
    // SAFETY: live descriptor, NUL-terminated name, `value` is value.len() bytes.
    let status = unsafe {
        libc::fsetxattr(
            file.as_raw_fd(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    if status != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}
