//! Lines for the host log: the VMM's stdout and stderr, which the app reads
//! as `<bundle>/logs/hvf/run.log`, a regular file in the same VM bundle as the
//! disk image (bridgevm-hvf-runtime vm_process.rs). `println!` panics when its
//! write fails, so when that volume fills, a line printed because it filled
//! would end the VMM instead of reporting the disk-full pause. These write and
//! drop whatever does not fit, and [`reserve_tail`] keeps space allocated past
//! the end of run.log so that the lines still land on a full volume.

use std::fmt::Display;
use std::io::{self, Write};
use std::os::fd::RawFd;

/// `println!` that drops the line when stdout cannot take it.
pub fn out(line: impl Display) {
    write_line(&mut io::stdout().lock(), line);
}

/// `eprintln!` that drops the line when stderr cannot take it.
pub fn err(line: impl Display) {
    write_line(&mut io::stderr().lock(), line);
}

fn write_line(out: &mut impl Write, line: impl Display) {
    let _ = writeln!(out, "{line}").and_then(|()| out.flush());
}

/// Keep about `bytes` allocated past the end of the regular file on `fd`, so
/// appends land in blocks the file already has and need no free space. On
/// APFS a log reserved this way takes its lines on a full volume, and the
/// reservation outlives a close (checked on a 16 MB APFS image: 1 MiB
/// reserved took 1.06 MiB of lines; an unreserved log, the rest of its last
/// block). Tops up to `bytes` once appends have used half. Ok(false): not a
/// regular file (a pipe, a terminal).
#[cfg(target_os = "macos")]
pub fn reserve_tail(fd: RawFd, bytes: u64) -> io::Result<bool> {
    let mut stat = std::mem::MaybeUninit::<libc::stat>::uninit();
    // SAFETY: a descriptor number and writable stat storage.
    if unsafe { libc::fstat(fd, stat.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstat succeeded and filled the storage.
    let stat = unsafe { stat.assume_init() };
    if stat.st_mode & libc::S_IFMT != libc::S_IFREG {
        return Ok(false);
    }
    let allocated = u64::try_from(stat.st_blocks).unwrap_or(0) * 512;
    let reserved = allocated.saturating_sub(u64::try_from(stat.st_size).unwrap_or(0));
    if reserved >= bytes / 2 {
        return Ok(true);
    }
    let mut store = libc::fstore_t {
        fst_flags: libc::F_ALLOCATEALL,
        fst_posmode: libc::F_PEOFPOSMODE,
        fst_offset: 0,
        fst_length: libc::off_t::try_from(bytes - reserved).unwrap_or(libc::off_t::MAX),
        fst_bytesalloc: 0,
    };
    // SAFETY: `store` is a valid fstore_t for the whole call.
    if unsafe { libc::fcntl(fd, libc::F_PREALLOCATE, &mut store) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(true)
}

#[cfg(not(target_os = "macos"))]
pub fn reserve_tail(_fd: RawFd, _bytes: u64) -> io::Result<bool> {
    Ok(false)
}

#[cfg(test)]
mod tests {
    use std::io;

    /// A volume with no space left.
    struct Full;

    impl io::Write for Full {
        fn write(&mut self, _: &[u8]) -> io::Result<usize> {
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        }

        fn flush(&mut self) -> io::Result<()> {
            Err(io::Error::from_raw_os_error(libc::ENOSPC))
        }
    }

    /// What the regular file at `path` has allocated past its end.
    #[cfg(target_os = "macos")]
    fn reserved(path: &std::path::Path) -> u64 {
        use std::os::unix::fs::MetadataExt;
        let meta = std::fs::metadata(path).unwrap();
        (meta.blocks() * 512).saturating_sub(meta.len())
    }

    /// run.log keeps its reservation topped up, never piled up: a second call,
    /// or a new probe generation on the same file, adds nothing while half is
    /// left. A pipe is left alone.
    #[test]
    #[cfg(target_os = "macos")] // elsewhere reserve_tail reserves nothing, by design
    fn the_log_reservation_is_topped_up_to_its_size() {
        use std::io::Write as _;
        use std::os::fd::AsRawFd;
        const MIB: u64 = 1 << 20;
        let path = std::env::temp_dir().join(format!("bv-log-reserve-{}", std::process::id()));
        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .truncate(true)
            .write(true)
            .open(&path)
            .unwrap();
        log.write_all(b"boot line\n").unwrap();
        assert!(super::reserve_tail(log.as_raw_fd(), 4 * MIB).unwrap());
        let first = reserved(&path);
        assert!((4 * MIB..5 * MIB).contains(&first), "{first}");
        assert!(super::reserve_tail(log.as_raw_fd(), 4 * MIB).unwrap());
        assert_eq!(reserved(&path), first, "nothing added while it is full");

        log.write_all(&vec![b'x'; 3 * MIB as usize]).unwrap();
        assert!(reserved(&path) < 2 * MIB);
        assert!(super::reserve_tail(log.as_raw_fd(), 4 * MIB).unwrap());
        let topped = reserved(&path);
        assert!((4 * MIB..5 * MIB).contains(&topped), "{topped}");

        let mut pipe = [0; 2];
        // SAFETY: a fresh pipe this test closes again.
        unsafe {
            assert_eq!(libc::pipe(pipe.as_mut_ptr()), 0);
            assert!(!super::reserve_tail(pipe[1], 4 * MIB).unwrap());
            libc::close(pipe[0]);
            libc::close(pipe[1]);
        }
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn a_line_the_full_volume_cannot_take_is_dropped() {
        super::write_line(&mut Full, "BVSTORAGE full errno=28 free=0 path=/");
        let mut log = Vec::new();
        super::write_line(&mut log, format_args!("BVPAUSE paused"));
        assert_eq!(log, b"BVPAUSE paused\n");
    }
}
