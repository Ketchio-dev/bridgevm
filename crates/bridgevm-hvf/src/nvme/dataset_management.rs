//! NVM Dataset Management (0x09) with the Deallocate attribute: Windows' TRIM.
//!
//! Windows deallocates a file's blocks when it deletes the file, and all free
//! space in its weekly retrim. On a raw write-back disk file the deallocated
//! ranges become holes (`F_PUNCHHOLE`), so the image gives the space back to
//! the Mac. Only the whole host file-system blocks inside a range are punched;
//! a partial block at either edge keeps its bytes.
//!
//! - Reads: Identify Namespace keeps DLFEAT at 0, so the guest is told nothing
//!   about what a deallocated block reads back. A punched block reads zeros,
//!   an edge or unpunched block its old data; NVMe 1.4 (6.7.1.1) allows both,
//!   and both stay the same until the guest writes the block. Windows never
//!   reads a trimmed block as data: NTFS marks the clusters free before it
//!   trims them and zero-fills past a file's valid data length itself.
//!   Promising zeros (DLFEAT 1, as QEMU does) would mean writing zeros over
//!   every edge and every disk that cannot punch.
//! - Nothing to punch: memory disks, read-only raw files (their writes live in
//!   the copy-on-write overlay) and file systems that cannot punch holes
//!   (HFS+ answers ENOTTY, others ENOTSUP) complete the command and keep their
//!   bytes. Deallocation is advisory, so the guest's data is the same.
//! - Durability: a punch changes the image file's allocation. Every FLUSH
//!   syncs that file, inline or on the flush worker through a duplicate of
//!   the same descriptor, and `F_BARRIERFSYNC` and `F_FULLFSYNC` commit its
//!   metadata with its data. A FLUSH after the DSM, the ordered mode's
//!   `F_FULLFSYNC` within a second and the shutdown sync therefore cover the
//!   punch as they cover a write; the DSM itself promises no durability. A
//!   host power loss before them can only undo the punch, and the old data
//!   then reads back, which DLFEAT 0 allows.
//! - Cost: the punches run inline in the I/O path, under the platform lock.
//!   The final report's `nvme dsm:` record ([`dsm_record`]) says how many
//!   commands ran and how long they took.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs::File;
use std::io;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering::Relaxed};
use std::time::Instant;

/// A range: context attributes (bytes 0-3), length in logical blocks (4-7,
/// a count, not 0-based) and starting LBA (8-15).
const RANGE_BYTES: usize = 16;

/// NR (CDW10 bits 7:0) counts the ranges 0-based.
const MAX_RANGES: usize = 256;

/// The Deallocate attribute (AD, CDW11 bit 2). IDR and IDW are only hints.
const ATTR_DEALLOCATE: u32 = 1 << 2;

/// A command longer than this is counted as long: a whole 20 ms audio buffer,
/// like a long platform-lock hold.
const LONG_US: u64 = 20_000;

impl NvmeController {
    /// NVM Dataset Management (0x09): punch each deallocated range.
    pub(crate) fn io_dataset_management(
        &mut self,
        cmd: &SubmissionEntry,
        mem: &mut dyn GuestMemoryMut,
    ) -> u16 {
        let Some(byte_len) = self.backend_for_nsid(cmd.nsid).map(DiskBackend::byte_len) else {
            return SC_INVALID_FIELD;
        };
        // The bits above NR are reserved: a value there asks for more than
        // 256 ranges.
        if cmd.cdw10 > 0xff {
            return SC_INVALID_FIELD_DNR;
        }
        if cmd.cdw11 & ATTR_DEALLOCATE == 0 {
            return SC_SUCCESS;
        }
        let started = Instant::now();
        let count = cmd.cdw10 as usize + 1;
        let mut list = [0u8; MAX_RANGES * RANGE_BYTES];
        let list = &mut list[..count * RANGE_BYTES];
        let mut ranges = [(0u64, 0u64); MAX_RANGES];
        let ranges = &mut ranges[..count];
        let mut outcome = Deallocated::default();
        let status = if !self.read_host_data(cmd, list, mem) {
            SC_INVALID_FIELD
        } else if !parse_ranges(list, byte_len, ranges) {
            outcome.rejected = true;
            SC_LBA_OUT_OF_RANGE | SC_DNR
        } else {
            let backend = self.backend_for_nsid_mut(cmd.nsid);
            outcome.ranges = ranges;
            outcome.punched = backend.map_or(0, |disk| disk.deallocate(ranges));
            SC_SUCCESS
        };
        DSM_STATS.note(&outcome, started);
        status
    }

    /// Copy the command's data (host to controller) out of guest memory
    /// through its PRP entries.
    fn read_host_data(
        &mut self,
        cmd: &SubmissionEntry,
        dst: &mut [u8],
        mem: &dyn GuestMemoryMut,
    ) -> bool {
        let mut spans = std::mem::take(&mut self.prp_spans_scratch);
        spans.clear();
        let mut ok = prp_spans_into(cmd, dst.len(), mem, &mut spans, &mut self.io_scratch);
        let mut offset = 0;
        for &(gpa, len) in &spans {
            ok = ok && mem.read_into(gpa, &mut dst[offset..offset + len]);
            offset += len;
        }
        spans.clear();
        self.prp_spans_scratch = spans;
        ok
    }
}

/// Decode every range of `list` into `out` as byte ranges, or `false` if one
/// ends past a `byte_len` namespace (its start plus length overflowing
/// included). Nothing is punched unless every range is valid.
fn parse_ranges(list: &[u8], byte_len: u64, out: &mut [(u64, u64)]) -> bool {
    for (range, raw) in out.iter_mut().zip(list.chunks_exact(RANGE_BYTES)) {
        let field = |at: usize, len: usize| {
            let mut bytes = [0u8; 8];
            bytes[..len].copy_from_slice(&raw[at..at + len]);
            u64::from_le_bytes(bytes)
        };
        let (blocks, slba) = (field(4, 4), field(8, 8));
        let lba = LBA_SIZE as u64;
        match slba
            .checked_add(blocks)
            .and_then(|end| end.checked_mul(lba))
        {
            // slba <= end, so its byte offset cannot overflow either.
            Some(end) if end <= byte_len => *range = (slba * lba, end),
            _ => return false,
        }
    }
    true
}

impl DiskBackend {
    /// Give the host blocks under `ranges` (byte ranges inside the disk) back
    /// to the host file system; returns the bytes punched. Only a raw
    /// write-back file has blocks to give back.
    pub(crate) fn deallocate(&mut self, ranges: &[(u64, u64)]) -> u64 {
        match self {
            Self::RawFile(disk) if disk.write_back => punch_ranges(&disk.file, ranges),
            _ => 0,
        }
    }
}

/// Punch the whole file-system blocks inside each range. A failure ends the
/// command's punching and is reported once per process; the guest's data is
/// unaffected either way.
fn punch_ranges(file: &File, ranges: &[(u64, u64)]) -> u64 {
    let block = match fs_block_size(file) {
        Ok(block) if block.is_power_of_two() => block,
        Ok(_) => return 0,
        Err(error) => {
            report_unpunched(&error);
            return 0;
        }
    };
    let mut punched = 0;
    for &(start, end) in ranges {
        let (from, to) = (start.div_ceil(block) * block, end / block * block);
        if from >= to {
            continue;
        }
        if let Err(error) = punch_hole(file, from, to - from) {
            report_unpunched(&error);
            break;
        }
        punched += to - from;
    }
    punched
}

/// The host file system cannot punch holes at all.
fn cannot_punch(error: &io::Error) -> bool {
    error.kind() == io::ErrorKind::Unsupported
        || error
            .raw_os_error()
            .is_some_and(|code| [libc::ENOTSUP, libc::EOPNOTSUPP, libc::ENOTTY].contains(&code))
}

fn report_unpunched(error: &io::Error) {
    static REPORTED: AtomicBool = AtomicBool::new(false);
    if REPORTED.swap(true, Relaxed) {
        return;
    }
    if cannot_punch(error) {
        eprintln!(
            "nvme: the disk image's file system cannot free blocks ({error}); \
             space Windows frees stays allocated on the Mac (APFS gives it back)"
        );
    } else {
        eprintln!(
            "nvme: freeing blocks in the disk image failed ({error}); \
             space Windows frees stays allocated on the Mac"
        );
    }
}

/// The host file system's block size (`statfs.f_bsize`), which
/// `F_PUNCHHOLE` offsets and lengths must be multiples of.
#[cfg(target_os = "macos")]
fn fs_block_size(file: &File) -> io::Result<u64> {
    use std::os::fd::AsRawFd;
    let mut stats = std::mem::MaybeUninit::<libc::statfs>::zeroed();
    // SAFETY: `file` owns a live descriptor and `stats` is writable.
    if unsafe { libc::fstatfs(file.as_raw_fd(), stats.as_mut_ptr()) } != 0 {
        return Err(io::Error::last_os_error());
    }
    // SAFETY: fstatfs succeeded, so it filled `stats`.
    Ok(u64::from(unsafe { stats.assume_init() }.f_bsize))
}

/// `F_PUNCHHOLE`: reads of the range return zeros and its blocks go back to
/// the file system; the file size does not change.
#[cfg(target_os = "macos")]
fn punch_hole(file: &File, offset: u64, len: u64) -> io::Result<()> {
    use std::os::fd::AsRawFd;
    #[cfg(test)]
    if let Some(code) = test_punch::errno() {
        return Err(io::Error::from_raw_os_error(code));
    }
    let off_t = |value: u64| {
        libc::off_t::try_from(value).map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    };
    let hole = libc::fpunchhole_t {
        fp_flags: 0,
        reserved: 0,
        fp_offset: off_t(offset)?,
        fp_length: off_t(len)?,
    };
    let hole = std::ptr::from_ref(&hole);
    // SAFETY: `file` owns a live descriptor; `hole` points to an initialised
    // fpunchhole_t that outlives the call, which only reads it.
    if unsafe { libc::fcntl(file.as_raw_fd(), libc::F_PUNCHHOLE, hole) } == -1 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// Other hosts (cross-compiled stubs) punch nothing.
#[cfg(not(target_os = "macos"))]
fn fs_block_size(_file: &File) -> io::Result<u64> {
    Err(io::ErrorKind::Unsupported.into())
}

#[cfg(not(target_os = "macos"))]
fn punch_hole(_file: &File, _offset: u64, _len: u64) -> io::Result<()> {
    Err(io::ErrorKind::Unsupported.into())
}

/// One DSM command, for [`DsmStats`].
#[derive(Default)]
struct Deallocated<'a> {
    rejected: bool,
    ranges: &'a [(u64, u64)],
    punched: u64,
}

/// The guest's Deallocate commands so far. A probe process runs one VM, so
/// these are its disks'.
struct DsmStats {
    commands: AtomicU64,
    rejected: AtomicU64,
    ranges: AtomicU64,
    bytes: AtomicU64,
    punched: AtomicU64,
    total_us: AtomicU64,
    max_us: AtomicU64,
    long: AtomicU64,
}

static DSM_STATS: DsmStats = DsmStats {
    commands: AtomicU64::new(0),
    rejected: AtomicU64::new(0),
    ranges: AtomicU64::new(0),
    bytes: AtomicU64::new(0),
    punched: AtomicU64::new(0),
    total_us: AtomicU64::new(0),
    max_us: AtomicU64::new(0),
    long: AtomicU64::new(0),
};

impl DsmStats {
    fn note(&self, command: &Deallocated, started: Instant) {
        let us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
        let bytes = command.ranges.iter().map(|(start, end)| end - start).sum();
        self.commands.fetch_add(1, Relaxed);
        self.rejected
            .fetch_add(u64::from(command.rejected), Relaxed);
        self.ranges.fetch_add(command.ranges.len() as u64, Relaxed);
        self.bytes.fetch_add(bytes, Relaxed);
        self.punched.fetch_add(command.punched, Relaxed);
        self.total_us.fetch_add(us, Relaxed);
        self.max_us.fetch_max(us, Relaxed);
        self.long.fetch_add(u64::from(us > LONG_US), Relaxed);
    }
}

/// The `nvme dsm:` record: Deallocate commands, those rejected for a range
/// outside the namespace, the valid ranges and their bytes, the bytes punched
/// on the host, and the commands' total and longest time and how many took
/// over 20 ms. They run under the platform lock, so each is a lock hold at
/// least that long.
pub fn dsm_record() -> String {
    let s = &DSM_STATS;
    let [commands, rejected, ranges, bytes, punched, total_us, max_us, long] = [
        &s.commands,
        &s.rejected,
        &s.ranges,
        &s.bytes,
        &s.punched,
        &s.total_us,
        &s.max_us,
        &s.long,
    ]
    .map(|n| n.load(Relaxed));
    format!(
        "nvme dsm: commands={commands} rejected={rejected} ranges={ranges} bytes={bytes} \
         punched_bytes={punched} total_us={total_us} max_us={max_us} gt_20ms={long}"
    )
}

/// Make this thread's punches fail with an errno, as a file system that
/// cannot punch does.
#[cfg(all(test, target_os = "macos"))]
pub(crate) mod test_punch {
    use std::cell::Cell;

    thread_local! {
        static ERRNO: Cell<Option<i32>> = const { Cell::new(None) };
    }

    pub(crate) fn fail_with(code: Option<i32>) {
        ERRNO.with(|errno| errno.set(code));
    }

    pub(crate) fn errno() -> Option<i32> {
        ERRNO.with(Cell::get)
    }
}
