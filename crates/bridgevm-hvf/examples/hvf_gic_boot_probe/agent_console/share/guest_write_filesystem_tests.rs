//! Filesystems that cannot represent POSIX permission bits (parent's decision,
//! 2026-10-23, revised the same day). On msdos and exfat every file reports
//! execute bits whatever chmod asks, so a guest write there is accepted only
//! with a quarantine mark that reads back. Anywhere else a leftover execute,
//! setuid, setgid or sticky bit refuses the write, and ENOTSUP still refuses
//! it. The filesystem type and the post-chmod mode are injected; no FAT or
//! exFAT image is mounted.

use super::*;
use bridgevm_hvf::host_entropy::HostEntropy;
use bridgevm_hvf::smccc_trng::{EntropyError, EntropySource};
use seal_host::{NativeSealHost, SealHost};
#[path = "guest_write_refusal_tests.rs"]
mod refusal;

/// How msdos and exfat report every file, whatever chmod asked.
const FAT_MODE: u32 = 0o100700;
/// A temporary whose chmod held.
const CLEAN_MODE: u32 = 0o100600;

enum Readback {
    /// The mark last set.
    Echo,
    Value(&'static [u8]),
    Errno(i32),
}

/// Injected observations that record what sealing asked of the host.
struct FakeHost {
    filesystem: &'static str,
    mode: u32,
    set_errno: Option<i32>,
    readback: Readback,
    /// Every mark sealing tried to set, including failed attempts.
    marks: Vec<Vec<u8>>,
    filesystem_queries: usize,
    readbacks: usize,
}

impl FakeHost {
    fn new(filesystem: &'static str, mode: u32) -> Self {
        Self {
            filesystem,
            mode,
            set_errno: None,
            readback: Readback::Echo,
            marks: Vec::new(),
            filesystem_queries: 0,
            readbacks: 0,
        }
    }

    /// Seal a new temporary (no replaced file) on the injected filesystem.
    fn seal(&mut self, entropy: &mut impl EntropySource) -> io::Result<()> {
        let f = Fixture::new();
        let file = File::create(f.root().join("t")).unwrap();
        policy::seal_with(None, &file, entropy, self)
    }
}

impl SealHost for FakeHost {
    fn filesystem(&mut self) -> io::Result<String> {
        self.filesystem_queries += 1;
        Ok(self.filesystem.to_owned())
    }
    fn mode(&mut self, _: &File) -> io::Result<u32> {
        Ok(self.mode)
    }
    fn set_mark(&mut self, _: &File, mark: &[u8]) -> io::Result<()> {
        self.marks.push(mark.to_vec());
        self.set_errno
            .map_or(Ok(()), |errno| Err(io::Error::from_raw_os_error(errno)))
    }
    fn get_mark(&mut self, _: &File) -> io::Result<Vec<u8>> {
        self.readbacks += 1;
        match self.readback {
            Readback::Echo => Ok(self.marks.last().cloned().unwrap_or_default()),
            Readback::Value(value) => Ok(value.to_vec()),
            Readback::Errno(errno) => Err(io::Error::from_raw_os_error(errno)),
        }
    }
}

/// A host CSPRNG that cannot produce bytes.
struct NoEntropy;

impl EntropySource for NoEntropy {
    fn fill(&mut self, _: &mut [u8]) -> Result<(), EntropyError> {
        Err(EntropyError::Unavailable)
    }
}

fn accepts_leftover_execute_bits_with_a_mark_that_reads_back(filesystem: &'static str) {
    let mut host = FakeHost::new(filesystem, FAT_MODE);
    host.seal(&mut HostEntropy::new()).unwrap();
    assert_eq!(host.marks.len(), 1, "{filesystem}: not marked once");
    assert!(host.marks[0].starts_with(b"0083;"), "{filesystem}");
    assert_eq!(host.readbacks, 1, "{filesystem}: mark not read back");
}

#[test]
fn msdos_accepts_leftover_execute_bits_with_a_mark_that_reads_back() {
    accepts_leftover_execute_bits_with_a_mark_that_reads_back("msdos");
}

#[test]
fn exfat_accepts_leftover_execute_bits_with_a_mark_that_reads_back() {
    accepts_leftover_execute_bits_with_a_mark_that_reads_back("exfat");
}

#[test]
fn msdos_refuses_when_the_mark_fails_or_does_not_read_back() {
    let other = b"0083;00000000;BridgeVM;00000000-0000-4000-8000-000000000000";
    let failures = [
        (Some(libc::EACCES), Readback::Echo),
        (Some(libc::ENOTSUP), Readback::Echo),
        (None, Readback::Value(other)),
        (None, Readback::Errno(libc::ENOATTR)),
    ];
    for (set_errno, readback) in failures {
        let mut host = FakeHost::new("msdos", FAT_MODE);
        (host.set_errno, host.readback) = (set_errno, readback);
        let error = host.seal(&mut HostEntropy::new()).unwrap_err();
        // Refused by the mark, not earlier by the execute bits.
        assert_eq!(host.marks.len(), 1, "{error}");
        if set_errno == Some(libc::ENOTSUP) {
            assert_eq!(error.kind(), io::ErrorKind::Unsupported, "{error}");
        }
    }
}

#[test]
fn msdos_and_exfat_still_refuse_setuid_setgid_or_sticky_bits() {
    for filesystem in ["msdos", "exfat"] {
        for mode in [0o104700, 0o102700, 0o101700] {
            let mut host = FakeHost::new(filesystem, mode);
            assert!(
                host.seal(&mut HostEntropy::new()).is_err(),
                "{filesystem} {mode:o}"
            );
            assert!(host.marks.is_empty(), "{filesystem} {mode:o} marked");
        }
    }
}

#[test]
fn any_other_filesystem_refuses_leftover_execute_bits() {
    for filesystem in ["smbfs", "nfs", "macfuse", "", "MSDOS", "exfat2", "msdos "] {
        let mut host = FakeHost::new(filesystem, FAT_MODE);
        let error = host.seal(&mut HostEntropy::new()).unwrap_err();
        assert_eq!(
            error.kind(),
            io::ErrorKind::PermissionDenied,
            "{filesystem:?}"
        );
        assert!(host.marks.is_empty(), "{filesystem:?} marked");
    }
}

#[test]
fn apfs_is_unchanged() {
    let mut host = FakeHost::new("apfs", FAT_MODE);
    assert!(host.seal(&mut HostEntropy::new()).is_err());
    assert!(host.marks.is_empty());
    // A clean mode never asks for the filesystem type or reads the mark back.
    let mut host = FakeHost::new("apfs", CLEAN_MODE);
    host.seal(&mut HostEntropy::new()).unwrap();
    assert_eq!(host.marks.len(), 1);
    assert_eq!((host.filesystem_queries, host.readbacks), (0, 0));
}

#[test]
fn csprng_failure_refuses_the_write_without_a_fallback_uuid() {
    for (filesystem, mode) in [("apfs", CLEAN_MODE), ("msdos", FAT_MODE)] {
        let mut host = FakeHost::new(filesystem, mode);
        assert!(host.seal(&mut NoEntropy).is_err());
        assert!(
            host.marks.is_empty(),
            "a mark was set without a CSPRNG UUID"
        );
    }
}

#[test]
fn share_filesystem_without_extended_attributes_refuses_the_write() {
    let mut host = FakeHost::new("apfs", CLEAN_MODE);
    host.set_errno = Some(libc::ENOTSUP);
    let error = host.seal(&mut HostEntropy::new()).unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::Unsupported);
    assert!(error.to_string().contains("extended attributes"), "{error}");
}

#[test]
fn a_real_temporary_that_keeps_execute_bits_off_fat_is_refused_unmarked() {
    let f = Fixture::new();
    let directory = File::open(f.root()).unwrap();
    let mut options = OpenOptions::new();
    options.write(true).create_new(true).mode(0o700);
    let file = options.open(f.root().join("t")).unwrap();
    let mut native = NativeSealHost {
        directory: &directory,
    };
    let filesystem = native.filesystem().unwrap();
    assert!(!filesystem.is_empty() && filesystem.bytes().all(|b| b.is_ascii_alphanumeric()));
    assert!(
        !matches!(filesystem.as_str(), "msdos" | "exfat"),
        "{filesystem}"
    );
    assert!(
        policy::seal(None, &file, &directory).is_err(),
        "sealed on {filesystem}"
    );
    assert!(native.get_mark(&file).is_err(), "marked on {filesystem}");
}
