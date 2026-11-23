//! The member files of a snapshot: opening, hashing, copying and free space.

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

/// Bytes copied per read/write when streaming a large disk image.
pub(super) const COPY_CHUNK: usize = 4 * 1024 * 1024;
const HASH_CHUNK: usize = 1024 * 1024;

/// Snapshot members must be regular files, including before their first read.
pub(super) fn open_regular(path: &Path) -> io::Result<File> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "snapshot member must be a regular file",
        ));
    }
    Ok(file)
}

/// Streaming SHA-256, so a disk image tens of gigabytes long is never read
/// into memory. The digest and the reported size come from the same bytes.
pub(super) fn sha256_reader(mut reader: impl Read) -> io::Result<(String, u64)> {
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; HASH_CHUNK];
    let mut bytes = 0;
    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
        bytes += n as u64;
    }
    let digest = hasher.finalize();
    let hash = digest.iter().map(|b| format!("{b:02x}")).collect();
    Ok((hash, bytes))
}

pub(super) fn sha256_file_and_size(path: &Path) -> io::Result<(String, u64)> {
    sha256_reader(open_regular(path)?)
}

pub(super) fn sha256_file(path: &Path) -> io::Result<String> {
    sha256_file_and_size(path).map(|(hash, _)| hash)
}

/// Copy `src` to `dst` and fsync the result, returning bytes written.
///
/// Streamed rather than read wholesale: a disk image is tens of gigabytes and
/// must not be brought into memory to be copied.
pub(super) fn copy_and_sync(src: &Path, dst: &Path) -> io::Result<u64> {
    copy_and_sync_using(src, dst, clone_file, available_bytes)
}

pub(super) fn copy_and_sync_using(
    src: &Path,
    dst: &Path,
    clone: impl FnOnce(&Path, &Path) -> Option<()>,
    available: impl FnOnce(&Path) -> Option<u64>,
) -> io::Result<u64> {
    // Clone first where the filesystem allows it. On APFS this is a
    // copy-on-write reference: the 64 GiB image clones in 2ms and adds no used
    // bytes, where copying it needs minutes and room for a second full copy.
    // That is what lets a restore run on a volume with less free space than
    // the image.
    if clone(src, dst).is_some() {
        let cloned = File::open(dst)?;
        // Still fsync: the clone is metadata, and the manifest is about to
        // claim these bytes are durable.
        cloned.sync_all()?;
        return Ok(cloned.metadata()?.len());
    }
    // Only a byte copy needs room for the source's logical size. Check before
    // creating the destination so a failed admission leaves no partial file.
    let needed = std::fs::metadata(src)?.len();
    let parent = dst
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    if available(parent).is_some_and(|bytes| needed > bytes) {
        return Err(io::Error::from_raw_os_error(libc::ENOSPC));
    }
    stream_and_sync(src, dst, |_| Ok(()))
}

/// The copy taken when cloning is refused, as across volumes. `after_chunk`
/// sees the running total; production observes nothing, and tests end the
/// copy there as a full volume or a killed process would.
pub(super) fn stream_and_sync(
    src: &Path,
    dst: &Path,
    mut after_chunk: impl FnMut(u64) -> io::Result<()>,
) -> io::Result<u64> {
    let mut input = File::open(src)?;
    let mut output = File::create(dst)?;
    let mut buf = vec![0u8; COPY_CHUNK];
    let mut total = 0u64;
    loop {
        let n = input.read(&mut buf)?;
        if n == 0 {
            break;
        }
        output.write_all(&buf[..n])?;
        total += n as u64;
        after_chunk(total)?;
    }
    output.sync_all()?;
    Ok(total)
}

/// Free bytes on the volume holding `path`, or None if it cannot be read.
///
/// None means "do not block the restore": a missing statfs is not evidence
/// that space is short, and the copy will fail loudly if it is.
pub(crate) fn available_bytes(path: &Path) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let c_path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    // SAFETY: c_path is a valid NUL-terminated string, and statfs only writes
    // into the buffer we hand it.
    let stat = unsafe {
        let mut stat: libc::statfs = std::mem::zeroed();
        if libc::statfs(c_path.as_ptr(), &mut stat) != 0 {
            return None;
        }
        stat
    };
    // f_bsize is u32 on macOS and i64 on Linux, so neither try_from nor a
    // plain From satisfies both targets; a cast is the portable spelling.
    (stat.f_bavail as u64).checked_mul(stat.f_bsize as u64)
}

/// Clone `src` to `dst` with APFS copy-on-write, or None if that is not
/// possible here.
///
/// Measured: cloning the 64 GiB Windows image takes 2ms and adds no used
/// bytes, against minutes and a second full copy for a byte-by-byte read and
/// write. None means "fall back to copying" -- a non-APFS volume, a
/// cross-volume destination, or any other refusal from the kernel.
#[cfg(target_os = "macos")]
pub(super) fn clone_file(src: &Path, dst: &Path) -> Option<()> {
    use std::os::unix::ffi::OsStrExt;
    let c_src = std::ffi::CString::new(src.as_os_str().as_bytes()).ok()?;
    let c_dst = std::ffi::CString::new(dst.as_os_str().as_bytes()).ok()?;
    // SAFETY: both strings are valid and NUL-terminated, and clonefile only
    // reads them.
    let rc = unsafe { libc::clonefile(c_src.as_ptr(), c_dst.as_ptr(), 0) };
    (rc == 0).then_some(())
}

/// No copy-on-write clone off macOS; callers fall back to copying.
#[cfg(not(target_os = "macos"))]
pub(super) fn clone_file(_src: &Path, _dst: &Path) -> Option<()> {
    None
}
