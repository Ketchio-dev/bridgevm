//! Import a QCOW2 (UTM, QEMU) or VHDX (Hyper-V) disk as the sparse raw image
//! the runtime boots: `snapshot_pair_cli convert-to-raw <src> <dst>`.
//!
//! The source file is untrusted. Its metadata is walked completely before
//! anything is written: every offset, size and count is checked against the
//! file length and fixed limits, and every refusal (a compressed cluster, a
//! backing file, a VHDX log to replay...) is met then, so a refused image
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
#[path = "disk_import_vhdx.rs"]
mod vhdx;

/// The largest virtual disk accepted. Windows on Arm images are 64-256 GiB.
pub const MAX_VIRTUAL_BYTES: u64 = 16 << 40;
/// The largest single read (and write) the copy issues.
const CHUNK_BYTES: usize = 8 << 20;
/// Zero detection granularity: the APFS block size, and qemu-img's default `-S`.
const ZERO_BLOCK: usize = 4096;

/// Container formats the import converts.
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
/// that tells the user what to do; the app shows it unchanged.
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
    Differencing,
    TooLarge,
    /// A version or a required feature this reader does not know.
    Unsupported(String),
    /// A structure that is out of range or inconsistent.
    Damaged(&'static str),
    NoSpace {
        needed: u64,
        available: u64,
    },
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        const GB: u64 = 1_000_000_000;
        match self {
            Refusal::NotAnImage => write!(f, "QCOW2나 VHDX 디스크가 아니니 디스크 파일을 다시 선택하세요."),
            Refusal::Compressed => write!(f, "이 QCOW2 디스크에는 압축된 영역이 있으니 UTM이나 qemu-img로 압축하지 않은 디스크로 변환한 뒤 다시 가져오세요."),
            Refusal::Encrypted => write!(f, "암호화된 QCOW2 디스크는 가져올 수 없으니 qemu-img로 암호화를 푼 디스크로 변환한 뒤 다시 가져오세요."),
            Refusal::BackingFile => write!(f, "이 디스크는 다른 디스크를 기반으로 하니 UTM이나 qemu-img로 하나의 디스크로 합친 뒤 다시 가져오세요."),
            Refusal::ExternalDataFile => write!(f, "이 QCOW2 디스크는 데이터를 별도 파일에 두니 qemu-img로 하나의 디스크로 변환한 뒤 다시 가져오세요."),
            Refusal::ExtendedL2 => write!(f, "이 QCOW2 디스크는 확장 L2(서브클러스터) 형식이니 qemu-img로 일반 디스크로 변환한 뒤 다시 가져오세요."),
            Refusal::Dirty => write!(f, "이 QCOW2 디스크는 사용 중이거나 제대로 닫히지 않았으니 UTM에서 VM을 완전히 종료한 뒤 다시 가져오세요."),
            Refusal::Corrupt => write!(f, "이 QCOW2 디스크에는 손상 표시가 있으니 qemu-img check -r all로 고친 뒤 다시 가져오세요."),
            Refusal::VhdxLog => write!(f, "이 VHDX에는 반영되지 않은 변경 기록이 남아 있으니 Windows에서 이 VHDX를 한 번 연결했다가 분리한 뒤 다시 시도하세요."),
            Refusal::Differencing => write!(f, "이 디스크는 다른 디스크를 기반으로 하는 차이 디스크이니 Hyper-V에서 부모 디스크와 병합한 뒤 다시 가져오세요."),
            Refusal::TooLarge => write!(f, "가상 크기가 16 TiB를 넘는 디스크는 가져올 수 없습니다."),
            Refusal::Unsupported(what) => write!(f, "이 디스크는 지원하지 않는 형식 기능({what})을 사용하니 qemu-img convert -O raw로 먼저 변환한 뒤 다시 가져오세요."),
            Refusal::Damaged(what) => write!(f, "디스크 이미지가 손상되었거나 구조가 올바르지 않으니 원래 VM 앱에서 디스크를 점검한 뒤 다시 시도하세요({what})."),
            Refusal::NoSpace { needed, available } => write!(
                f,
                "변환한 디스크를 저장할 공간이 부족하니 대상 볼륨에 {} GB 이상을 비운 뒤 다시 시도하세요(필요 {} GB, 남은 공간 {} GB).",
                (needed - available).div_ceil(GB),
                needed.div_ceil(GB),
                available / GB
            ),
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
            ConvertError::Io(error) => write!(f, "디스크를 변환하지 못했습니다: {error}"),
        }
    }
}

impl std::error::Error for ConvertError {}

type Result<T> = std::result::Result<T, ConvertError>;

/// One stretch of the virtual disk: data at `host` in the image file, or a
/// hole (unallocated or zero) when `host` is None. Runs arrive in order and
/// tile `[0, virtual size)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Run {
    virt: u64,
    len: u64,
    host: Option<u64>,
}

/// The parsed metadata of a source image.
enum Image {
    Qcow2(qcow2::Qcow2),
    Vhdx(vhdx::Vhdx),
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
            None => Err(Refusal::NotAnImage.into()),
        }
    }

    fn virtual_size(&self) -> u64 {
        match self {
            Image::Qcow2(image) => image.size,
            Image::Vhdx(image) => image.size,
        }
    }

    fn walk(&self, file: &File, sink: &mut dyn FnMut(Run) -> Result<()>) -> Result<()> {
        match self {
            Image::Qcow2(image) => image.walk(file, sink),
            Image::Vhdx(image) => image.walk(file, sink),
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
        if run.host.is_some() {
            data_bytes += run.len;
        }
        Ok(())
    })?;
    let parent = match dst.parent() {
        Some(parent) if !parent.as_os_str().is_empty() => parent,
        _ => Path::new("."),
    };
    if let Some(available) = crate::snapshot_pair::free_space::available_bytes(parent) {
        if available < data_bytes {
            return Err(Refusal::NoSpace {
                needed: data_bytes,
                available,
            }
            .into());
        }
    }

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
        let Some(mut host) = run.host else {
            return self.flush();
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
        self.done += len;
        let percent = (self.done.saturating_mul(100) / self.total.max(1)).min(99) as u8;
        if percent > self.percent {
            self.percent = percent;
            (self.progress)(percent);
        }
        Ok(())
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
