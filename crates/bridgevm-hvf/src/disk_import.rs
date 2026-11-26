//! Import QCOW2, fixed Hyper-V VHD and VHDX disks as the sparse raw image
//! the runtime boots: `snapshot_pair_cli convert-to-raw <src> <dst>`.
//!
//! The source file is untrusted. Its metadata is walked completely before
//! anything is written: every offset, size and count is checked against the
//! file length and fixed limits, and every metadata refusal (a backing file, a
//! VHDX log to replay...) is met then, so a refused image
//! leaves nothing behind. The second walk copies: runs of data contiguous in
//! both the image and the file become one large read, and only non-zero 4 KiB
//! blocks are written, so unallocated, zero and all-zero clusters stay holes.

use std::fmt;
use std::fs::File;
use std::io::{self, Write};
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::process::ExitCode;

#[path = "disk_import_output.rs"]
mod output;
#[path = "disk_import_qcow2.rs"]
mod qcow2;
#[path = "disk_import_vhd.rs"]
mod vhd;
#[path = "disk_import_vhdx.rs"]
mod vhdx;

/// The largest virtual disk accepted. Windows on Arm images are 64-256 GiB.
pub const MAX_VIRTUAL_BYTES: u64 = 16 << 40;
/// The largest single read (and write) the copy issues.
const CHUNK_BYTES: usize = 8 << 20;
/// Zero detection granularity: the APFS block size, and qemu-img's default `-S`.
const ZERO_BLOCK: usize = 4096;

/// Container formats identifiable by their leading signature. Fixed VHD has only a footer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Qcow2,
    Vhdx,
}

impl Format {
    /// The format named by the first bytes of a file, if it is one we convert.
    pub fn detect(head: &[u8]) -> Option<Self> {
        if head.starts_with(b"QFI\xfb") {
            Some(Self::Qcow2)
        } else if head.starts_with(b"vhdxfile") {
            Some(Self::Vhdx)
        } else {
            None
        }
    }
}

/// Why an image cannot be converted. Each displays as one plain sentence
/// with recovery guidance. The app omits diagnostic details from its messages.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    NotAnImage,
    Compressed,
    Encrypted,
    BackingFile,
    ExternalDataFile,
    ExtendedL2,
    Dirty,
    Corrupt,
    VhdxLog,
    DynamicVhd,
    Differencing,
    /// The runtime's disks have 512-byte logical sectors; a 4 KiB-sector GPT would not be found.
    Sector4K,
    TooLarge,
    /// A version or a required feature this reader does not know.
    Unsupported(String),
    /// A structure that is out of range or inconsistent.
    Damaged(&'static str),
    NoSpace,
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Refusal::NotAnImage => write!(f, "Choose a QCOW2, fixed-size VHD, or VHDX disk to convert."),
            Refusal::Compressed => write!(f, "This QCOW2 uses unsupported compression. Export it as RAW from the original VM app and import the copy."),
            Refusal::Encrypted => write!(f, "This QCOW2 disk is encrypted. Use qemu-img to export a decrypted copy and import it."),
            Refusal::BackingFile => write!(f, "This disk depends on a backing file. Export a standalone disk from the original VM app and import the copy."),
            Refusal::ExternalDataFile => write!(f, "This QCOW2 stores data in a separate file. Use qemu-img to export a standalone disk and import the copy."),
            Refusal::ExtendedL2 => write!(f, "This QCOW2 uses extended L2 tables. Use qemu-img to convert it to RAW and import the copy."),
            Refusal::Dirty => write!(f, "This QCOW2 is still in use or was closed unexpectedly. Shut down the source VM cleanly and try again."),
            Refusal::Corrupt => write!(f, "This QCOW2 is marked as corrupt. Check a copy with qemu-img check and import it after repair."),
            Refusal::VhdxLog => write!(f, "This VHDX has pending changes. Attach it in Windows, detach it cleanly, then try again."),
            Refusal::DynamicVhd => write!(f, "Convert the dynamic VHD to a fixed-size VHD or VHDX in Hyper-V, then import the converted disk."),
            Refusal::Differencing => write!(f, "This differencing disk depends on a parent disk. Merge the chain in Hyper-V, then import the merged disk."),
            Refusal::Sector4K => write!(f, "This VHDX uses 4 KB logical sectors. Import Windows from a disk that uses 512-byte logical sectors."),
            Refusal::TooLarge => write!(f, "Choose a disk with a virtual size of 16 TiB or less."),
            Refusal::Unsupported(what) => write!(f, "This disk uses an unsupported feature ({what}). Export it as RAW from the original VM app and import the copy."),
            Refusal::Damaged(what) => write!(f, "The disk image is damaged ({what}). Check it in the original VM app and try again."),
            Refusal::NoSpace => write!(f, "No space left on device. Free space on the destination volume and try again."),
        }
    }
}

#[derive(Debug)]
pub enum ConvertError {
    Refused(Refusal),
    Io(io::Error),
}

impl From<io::Error> for ConvertError {
    fn from(error: io::Error) -> Self {
        if error.raw_os_error() == Some(libc::ENOSPC) {
            return ConvertError::Refused(Refusal::NoSpace);
        }
        ConvertError::Io(error)
    }
}

impl From<Refusal> for ConvertError {
    fn from(refusal: Refusal) -> Self {
        ConvertError::Refused(refusal)
    }
}

impl fmt::Display for ConvertError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConvertError::Refused(refusal) => refusal.fmt(f),
            ConvertError::Io(error) => write!(f, "Could not convert the disk: {error}"),
        }
    }
}

impl std::error::Error for ConvertError {}

type Result<T> = std::result::Result<T, ConvertError>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RunSource {
    Hole,
    Data(u64),
    Deflate(qcow2::CompressedCluster),
}

/// Runs arrive in virtual disk order and tile `[0, virtual size)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    virt: u64,
    len: u64,
    source: RunSource,
}

/// The parsed metadata of a source image.
enum Image {
    Qcow2(qcow2::Qcow2),
    Vhdx(vhdx::Vhdx),
    FixedVhd(u64),
}

impl Image {
    fn open(file: &File) -> Result<Self> {
        let file_len = file.metadata()?.len();
        let mut head = [0u8; 8];
        if file_len < head.len() as u64 {
            return Err(Refusal::NotAnImage.into());
        }
        file.read_exact_at(&mut head, 0)?;
        match Format::detect(&head) {
            Some(Format::Qcow2) => Ok(Image::Qcow2(qcow2::Qcow2::open(file, file_len)?)),
            Some(Format::Vhdx) => Ok(Image::Vhdx(vhdx::Vhdx::open(file, file_len)?)),
            None => Ok(Image::FixedVhd(vhd::fixed_size(file, file_len)?)),
        }
    }

    fn virtual_size(&self) -> u64 {
        match self {
            Image::Qcow2(image) => image.size,
            Image::Vhdx(image) => image.size,
            Image::FixedVhd(size) => *size,
        }
    }

    fn walk(&self, file: &File, sink: &mut dyn FnMut(Run) -> Result<()>) -> Result<()> {
        match self {
            Image::Qcow2(image) => image.walk(file, sink),
            Image::Vhdx(image) => image.walk(file, sink),
            Image::FixedVhd(size) => sink(Run {
                virt: 0,
                len: *size,
                source: RunSource::Data(0),
            }),
        }
    }
}

/// Convert `src` to a sparse raw image at `dst`, which must not exist.
/// `progress` sees whole percents of the data copied, rising, ending at 100.
/// Returns the raw image's length (the virtual disk size).
pub fn convert_to_raw(src: &Path, dst: &Path, mut progress: impl FnMut(u8)) -> Result<u64> {
    let file = File::open(src)?;
    if !file.metadata()?.is_file() {
        return Err(Refusal::NotAnImage.into());
    }
    let image = Image::open(&file)?;
    let size = image.virtual_size();

    // Pass 1: every L2 table or BAT entry is read and checked, and the data
    // to copy is counted, before anything exists at the destination.
    let mut data_bytes = 0u64;
    image.walk(&file, &mut |run| {
        if run.source != RunSource::Hole {
            data_bytes += run.len;
        }
        Ok(())
    })?;
    // Allocated source blocks can contain zeros that occupy no space in the
    // sparse destination. Report ENOSPC only when the filesystem returns it.
    // Pass 2: copy.
    let out = output::RawOutput::create(dst, size)?;
    let mut copier = Copier {
        src: &file,
        out: &out,
        buf: vec![0; CHUNK_BYTES],
        pending: None,
        done: 0,
        total: data_bytes,
        percent: 0,
        progress: &mut progress,
    };
    image.walk(&file, &mut |run| copier.add(run))?;
    copier.flush()?;
    out.publish()?;
    progress(100);
    Ok(size)
}

/// Coalesces data runs into large reads and writes their non-zero blocks.
struct Copier<'a> {
    src: &'a File,
    out: &'a output::RawOutput,
    buf: Vec<u8>,
    /// (virtual offset, file offset, length) waiting to be read.
    pending: Option<(u64, u64, u64)>,
    done: u64,
    total: u64,
    percent: u8,
    progress: &'a mut dyn FnMut(u8),
}

impl Copier<'_> {
    fn add(&mut self, run: Run) -> Result<()> {
        let mut host = match run.source {
            RunSource::Hole => return self.flush(),
            RunSource::Data(offset) => offset,
            RunSource::Deflate(cluster) => {
                self.flush()?;
                let data = cluster.decode(self.src, &mut self.buf)?;
                write_nonzero(self.out, &data[..run.len as usize], run.virt)?;
                self.advance(run.len);
                return Ok(());
            }
        };
        let (mut virt, mut len) = (run.virt, run.len);
        while len > 0 {
            if let Some((pv, ph, pl)) = self.pending {
                if pv + pl == virt && ph + pl == host && pl < CHUNK_BYTES as u64 {
                    let take = len.min(CHUNK_BYTES as u64 - pl);
                    self.pending = Some((pv, ph, pl + take));
                    (virt, host, len) = (virt + take, host + take, len - take);
                    continue;
                }
                self.flush()?;
            }
            let take = len.min(CHUNK_BYTES as u64);
            self.pending = Some((virt, host, take));
            (virt, host, len) = (virt + take, host + take, len - take);
        }
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        let Some((virt, host, len)) = self.pending.take() else {
            return Ok(());
        };
        let buf = &mut self.buf[..len as usize];
        read_padded(self.src, buf, host)?;
        write_nonzero(self.out, buf, virt)?;
        self.advance(len);
        Ok(())
    }

    fn advance(&mut self, len: u64) {
        self.done += len;
        let percent = (self.done.saturating_mul(100) / self.total.max(1)).min(99) as u8;
        if percent > self.percent {
            self.percent = percent;
            (self.progress)(percent);
        }
    }
}

/// Fill `buf` from `offset`; bytes past the end of the file read as zero, the
/// way QEMU reads the tail of a cluster that the image file does not cover.
fn read_padded(file: &File, buf: &mut [u8], offset: u64) -> io::Result<()> {
    let mut filled = 0;
    while filled < buf.len() {
        match file.read_at(&mut buf[filled..], offset + filled as u64) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
            Err(error) => return Err(error),
        }
    }
    buf[filled..].fill(0);
    Ok(())
}

/// Write the non-zero 4 KiB blocks of `buf`, merged into runs, at `virt`.
fn write_nonzero(out: &output::RawOutput, buf: &[u8], virt: u64) -> io::Result<()> {
    let mut at = 0;
    while at < buf.len() {
        while at < buf.len() && is_zero(block(buf, at)) {
            at += ZERO_BLOCK;
        }
        let start = at;
        while at < buf.len() && !is_zero(block(buf, at)) {
            at += ZERO_BLOCK;
        }
        let end = at.min(buf.len());
        if end > start {
            out.write_at(&buf[start..end], virt + start as u64)?;
        }
    }
    Ok(())
}

fn block(buf: &[u8], at: usize) -> &[u8] {
    &buf[at..buf.len().min(at + ZERO_BLOCK)]
}

fn is_zero(bytes: &[u8]) -> bool {
    let mut words = bytes.chunks_exact(8);
    let any = words.by_ref().fold(0u64, |acc, w| {
        acc | u64::from_ne_bytes(w.try_into().unwrap())
    });
    any == 0 && words.remainder().iter().all(|&b| b == 0)
}

/// `snapshot_pair_cli convert-to-raw <src> <dst>`: `progress <percent>` lines
/// on stderr while copying, then `disk_bytes <n>` on stdout. A refusal is one
/// sentence on stderr and exit status 1.
pub fn command(arguments: &[String]) -> ExitCode {
    let [src, dst] = arguments else {
        return ExitCode::from(2);
    };
    let result = convert_to_raw(Path::new(src), Path::new(dst), |percent| {
        let _ = writeln!(io::stderr().lock(), "progress {percent}");
    });
    match result {
        Ok(bytes) => match writeln!(io::stdout().lock(), "disk_bytes {bytes}") {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        },
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}

/// Big-endian and little-endian field readers over a checked slice.
fn be32(b: &[u8], at: usize) -> u32 {
    u32::from_be_bytes(b[at..at + 4].try_into().unwrap())
}
fn be64(b: &[u8], at: usize) -> u64 {
    u64::from_be_bytes(b[at..at + 8].try_into().unwrap())
}
fn le16(b: &[u8], at: usize) -> u16 {
    u16::from_le_bytes(b[at..at + 2].try_into().unwrap())
}
fn le32(b: &[u8], at: usize) -> u32 {
    u32::from_le_bytes(b[at..at + 4].try_into().unwrap())
}
fn le64(b: &[u8], at: usize) -> u64 {
    u64::from_le_bytes(b[at..at + 8].try_into().unwrap())
}

/// `offset + len <= file_len` without overflow.
fn within(offset: u64, len: u64, file_len: u64) -> bool {
    offset.checked_add(len).is_some_and(|end| end <= file_len)
}

#[cfg(test)]
#[path = "disk_import_fixtures.rs"]
mod fixtures;
#[cfg(test)]
#[path = "disk_import_qemu_tests.rs"]
mod qemu_tests;
#[cfg(test)]
#[path = "disk_import_tests.rs"]
mod tests;
