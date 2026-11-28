//! The member files of a snapshot: opening, hashing, copying and free space.

use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the handler `stop_copies_on_signals` installs. Dying of the signal left an export's
/// partial copy (tens of GB on a backup drive) in its hidden staging folder.
static STOP_REQUESTED: AtomicBool = AtomicBool::new(false);

/// For the helper's create, restore, import and convert commands: SIGINT and SIGTERM (the app's
/// and the CLI's cancel) stop the copy or hash in progress at its next chunk, so the attempt's own
/// error path removes its staging and the command fails. A signal after the copies lets it finish.
pub fn stop_copies_on_signals() {
    extern "C" fn request_stop(_: libc::c_int) {
        STOP_REQUESTED.store(true, Ordering::Relaxed);
    }
    let handler = request_stop as extern "C" fn(libc::c_int);
    for signal in [libc::SIGINT, libc::SIGTERM] {
        // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
        unsafe { libc::signal(signal, handler as libc::sighandler_t) };
    }
}

pub(crate) fn stop_requested() -> io::Result<()> {
    #[cfg(test)]
    let requested = stop_for_test::requested() || STOP_REQUESTED.load(Ordering::Relaxed);
    #[cfg(not(test))]
    let requested = STOP_REQUESTED.load(Ordering::Relaxed);
    if requested {
        return Err(io::Error::other("stopped by a signal"));
    }
    Ok(())
}

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
        stop_requested()?;
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
    // Clone first where the filesystem allows it. On APFS this is a
    // copy-on-write reference: the 64 GiB image clones in 2ms and adds no used
    // bytes, where copying it needs minutes and room for a second full copy.
    // That is what lets a restore run on a volume with less free space than
    // the image.
    if clone_file(src, dst).is_some() {
        let cloned = File::open(dst)?;
        // Still fsync: the clone is metadata, and the manifest is about to
        // claim these bytes are durable.
        cloned.sync_all()?;
        return Ok(cloned.metadata()?.len());
    }
    // No free-space check against the logical size: the copy stays sparse, so a
    // 256 GiB disk holding 40 GB needs about 40 GB. A volume that does fill up
    // fails the write with ENOSPC, and every caller removes its partial copy.
    stream_and_sync(src, dst, |_| Ok(()))
}

/// The copy taken when cloning is refused, as across volumes. `after_chunk`
/// sees the running total; production observes nothing, and tests end the
/// copy there as a full volume or a killed process would.
///
/// All-zero chunks are skipped, not written, so they stay holes in the copy;
/// holes read back as zeros, so the copy's bytes and hash match the source's.
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
        if crate::disk_import::is_zero(&buf[..n]) {
            output.seek(SeekFrom::Current(n as i64))?;
        } else {
            output.write_all(&buf[..n])?;
        }
        total += n as u64;
        after_chunk(total)?;
        stop_requested()?;
    }
    // A seek alone does not extend the file: a trailing hole needs the length set.
    output.set_len(total)?;
    output.sync_all()?;
    Ok(total)
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

/// The flag is per process; tests set a per-thread one so parallel tests are unaffected.
#[cfg(test)]
pub(crate) mod stop_for_test {
    use std::cell::Cell;
    thread_local!(static REQUESTED: Cell<bool> = const { Cell::new(false) });
    pub(crate) fn set(requested: bool) {
        REQUESTED.with(|flag| flag.set(requested));
    }
    pub(super) fn requested() -> bool {
        REQUESTED.with(Cell::get)
    }
}
