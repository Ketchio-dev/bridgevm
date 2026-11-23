//! Selection occurs before VM creation and every media read in the native probe.
//!
//! One runtime owner captures the media policies at acquisition and refuses
//! cross-slot outputs before they can replace another media generation. Fresh
//! staged files are owned before publication, and uncertain writes stay owned
//! until the owner is dropped.

use super::{resolve_root, LockedPair};
use crate::media::{MediaWrite, MediaWriteKind, VirtBootMediaConfig, WritableMedia};
use crate::media_lease::MediaLease;
use std::collections::BTreeSet;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Clone, Copy, Debug)]
pub enum RuntimeMediaSlot {
    Vars,
    Primary,
    Target,
}
struct Policies {
    slots: [Option<WritableMedia>; 3],
    originals: [Option<PathBuf>; 3],
}
impl Policies {
    fn capture(media: &VirtBootMediaConfig) -> Self {
        Self {
            slots: [None, None, None],
            originals: [
                Some(media.flash_vars.path.clone()),
                media.nvme_disk.as_ref().map(|m| m.path.clone()),
                media.nvme_target.as_ref().map(|m| m.path.clone()),
            ],
        }
    }
    fn slot(&self, slot: RuntimeMediaSlot) -> io::Result<WritableMedia> {
        self.validate()?;
        self.slots[slot as usize]
            .clone()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "runtime media slot absent"))
    }
    fn validate(&self) -> io::Result<()> {
        for (index, slot) in self.slots.iter().enumerate() {
            let Some(slot) = slot else { continue };
            for output in slot
                .snapshot_path
                .iter()
                .chain(slot.write_back.then_some(&slot.path))
            {
                for other in 0..self.slots.len() {
                    if other == index {
                        continue;
                    }
                    let input = self.slots[other].as_ref();
                    for path in self.originals[other]
                        .iter()
                        .chain(input.map(|s| &s.path))
                        .chain(input.and_then(|s| s.snapshot_path.as_ref()))
                    {
                        if same_output(output, path)? {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "runtime media output overlaps another slot",
                            ));
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

fn capture_slots(mut lease: RuntimeLease, media: &VirtBootMediaConfig) -> io::Result<RuntimeLease> {
    lease.policies.slots = [
        Some(media.flash_vars.clone()),
        media.nvme_disk.clone(),
        media.nvme_target.clone(),
    ];
    if media.nvme_disk.is_some() || media.nvme_target.is_some() {
        for slot in lease.policies.slots.iter().flatten() {
            lease.retained.insert(fs::canonicalize(&slot.path)?);
        }
    }
    if let Some(pair) = &lease._pair {
        lease
            .retained
            .extend([pair.disk.clone(), pair.vars.clone()]);
    }
    lease.policies.validate()?;
    Ok(lease)
}

/// The captured runtime owner, with once-per-raw-slot source lifetime pins.
pub struct RuntimeLease {
    _pair: Option<LockedPair>,
    _logical: MediaLease,
    policies: Policies,
    retained: BTreeSet<PathBuf>,
    raw_source_pinned: [bool; 2],
}

impl RuntimeLease {
    fn new(media: &VirtBootMediaConfig) -> io::Result<Self> {
        Ok(Self {
            _pair: None,
            _logical: MediaLease::acquire([])?,
            policies: Policies::capture(media),
            retained: BTreeSet::new(),
            raw_source_pinned: [false; 2],
        })
    }
}

/// Select and capture the media policies protected by one runtime owner.
pub fn acquire(media: &mut VirtBootMediaConfig) -> io::Result<RuntimeLease> {
    let mut lease = RuntimeLease::new(media)?;
    let disks: Vec<_> = [&media.nvme_disk, &media.nvme_target]
        .into_iter()
        .flatten()
        .collect();
    if disks.is_empty() {
        return capture_slots(lease, media);
    }
    if disks.len() == 2 {
        lease._logical = MediaLease::acquire([
            disks[0].path.as_path(),
            disks[1].path.as_path(),
            media.flash_vars.path.as_path(),
        ])?;
        let vars = fs::canonicalize(&media.flash_vars.path)?;
        for disk in disks {
            let root = resolve_root(&fs::canonicalize(&disk.path)?, &vars)?;
            match fs::symlink_metadata(root) {
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
                Ok(_) => {
                    return Err(io::Error::other(
                        "managed shared-vars selection is ambiguous with two disks",
                    ))
                }
            }
        }
        return capture_slots(lease, media);
    }
    let original_disk = fs::canonicalize(&disks[0].path)?;
    let original_vars = fs::canonicalize(&media.flash_vars.path)?;
    let pair = LockedPair::open(&original_disk, &original_vars)?;
    let (disk, vars) = pair.paths()?;
    let slot = media
        .nvme_disk
        .as_mut()
        .or(media.nvme_target.as_mut())
        .unwrap();
    slot.path = disk;
    media.flash_vars.path = vars;
    lease._pair = Some(pair);
    capture_slots(lease, media)
}

impl RuntimeLease {
    /// Atomically persist a captured media policy while retaining ownership of
    /// every runtime input and output until this owner is dropped.
    pub fn persist(&mut self, slot: RuntimeMediaSlot, bytes: &[u8]) -> io::Result<Vec<MediaWrite>> {
        self.persist_using(slot, bytes, |staged, destination| {
            fs::rename(staged, destination)?;
            fs::File::open(destination.parent().unwrap())?.sync_all()
        })
    }

    fn persist_using(
        &mut self,
        slot: RuntimeMediaSlot,
        bytes: &[u8],
        mut publish: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<Vec<MediaWrite>> {
        let policy = self.policies.slot(slot)?;
        policy.persist_with(bytes, |path, bytes| {
            self.write_owned(path, bytes, &mut publish)
        })
    }

    fn owner(&mut self) -> &mut MediaLease {
        match self._pair.as_mut() {
            Some(pair) => &mut pair._lease,
            None => &mut self._logical,
        }
    }

    fn write_owned(
        &mut self,
        path: &Path,
        bytes: &[u8],
        publish: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<()> {
        self.write_owned_with(path, |out| out.write_all(bytes), publish)
    }

    fn write_owned_with<T>(
        &mut self,
        path: &Path,
        write: impl FnOnce(&mut dyn Write) -> io::Result<T>,
        publish: &mut impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<T> {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let name = path
            .file_name()
            .ok_or_else(|| io::Error::other("media output has no filename"))?;
        let destination = fs::canonicalize(parent)?.join(name);
        self.owner().extend([destination.as_path()])?;
        // Keep this path even if publication later returns an uncertain error.
        self.retained.insert(destination.clone());
        let mut staged = StagedFile::create(&destination)?;
        self.owner().extend([staged.path.as_path()])?;
        let result = write(&mut staged.file)?;
        staged.file.sync_all()?;
        publish(&staged.path, &destination)?;
        let retained: Vec<_> = self.retained.iter().cloned().collect();
        self.owner()
            .replace(retained.iter().map(PathBuf::as_path))?;
        Ok(result)
    }

    /// Writes only the configured snapshot, never write-back media. The producer
    /// streams bytes; this owner controls staging, sync, publication and leases.
    pub fn export_snapshot(
        &mut self,
        slot: RuntimeMediaSlot,
        write: impl FnOnce(&mut dyn Write) -> io::Result<u64>,
    ) -> io::Result<Option<MediaWrite>> {
        self.export_snapshot_using(slot, write, |staged, destination| {
            fs::rename(staged, destination)?;
            fs::File::open(destination.parent().unwrap())?.sync_all()
        })
    }

    fn export_snapshot_using(
        &mut self,
        slot: RuntimeMediaSlot,
        write: impl FnOnce(&mut dyn Write) -> io::Result<u64>,
        mut publish: impl FnMut(&Path, &Path) -> io::Result<()>,
    ) -> io::Result<Option<MediaWrite>> {
        let policy = self.policies.slot(slot)?;
        let Some(path) = policy.snapshot_path else {
            return Ok(None);
        };
        if let RuntimeMediaSlot::Primary | RuntimeMediaSlot::Target = slot {
            let index = slot as usize - 1;
            if !self.raw_source_pinned[index] {
                self.owner().pin([policy.path.as_path()])?;
                // Preserve the open source, not successive self-export generations.
                self.raw_source_pinned[index] = true;
            }
        }
        let bytes = self.write_owned_with(
            &path,
            |out| {
                usize::try_from(write(out)?)
                    .map_err(|_| io::Error::other("snapshot byte count exceeds host size"))
            },
            &mut publish,
        )?;
        Ok(Some(MediaWrite {
            kind: MediaWriteKind::Snapshot,
            path,
            bytes,
        }))
    }
}

/// Exclusively created staging; cleanup only removes its own surviving inode.
struct StagedFile {
    path: PathBuf,
    file: File,
}

impl StagedFile {
    fn create(destination: &Path) -> io::Result<Self> {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let parent = destination.parent().unwrap();
        for _ in 0..128 {
            let sequence = NEXT.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(
                ".bridgevm-write-{}-{sequence}.tmp",
                std::process::id()
            ));
            if path
                .file_name()
                .unwrap()
                .as_bytes()
                .eq_ignore_ascii_case(destination.file_name().unwrap().as_bytes())
            {
                continue;
            }
            match OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&path)
            {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "no free media staging name",
        ))
    }
}

impl Drop for StagedFile {
    fn drop(&mut self) {
        // Publication may have renamed the file even when it returned Err.
        // Clean only our own remaining inode, never a replacement at this name.
        if let (Ok(path), Ok(file)) = (fs::symlink_metadata(&self.path), self.file.metadata()) {
            if (path.dev(), path.ino()) == (file.dev(), file.ino()) {
                let _ = fs::remove_file(&self.path);
            }
        }
    }
}

// Resolve missing output tails too, while detecting existing hard-link aliases.

fn resolved(path: &Path) -> io::Result<PathBuf> {
    match fs::canonicalize(path) {
        Ok(path) => Ok(path),
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let name = path
                .file_name()
                .ok_or_else(|| io::Error::other("media output has no filename"))?;
            let parent = path
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            Ok(resolved(parent)?.join(name))
        }
        Err(error) => Err(error),
    }
}
fn same_output(left: &Path, right: &Path) -> io::Result<bool> {
    let (left, right) = (resolved(left)?, resolved(right)?);
    if left == right || case_alias(&left, &right)? {
        return Ok(true);
    }
    let identity = |path: &Path| match fs::metadata(path) {
        Ok(metadata) => Ok(Some((metadata.dev(), metadata.ino()))),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    };
    Ok(matches!((identity(&left)?, identity(&right)?), (Some(left), Some(right)) if left == right))
}

#[cfg(not(target_os = "macos"))]
fn case_alias(_: &Path, _: &Path) -> io::Result<bool> {
    Ok(false)
}

#[cfg(target_os = "macos")]
use unicode::case_alias;

#[cfg(target_os = "macos")]
mod unicode {
    //! Conservative Unicode alias comparison for absent names in one resolved parent.
    use std::ffi::{c_void, OsStr};
    use std::os::unix::ffi::OsStrExt;
    use std::{fs, io, path::Path};

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFStringCreateWithBytes(
            allocator: *const c_void,
            bytes: *const u8,
            count: isize,
            encoding: u32,
            external: u8,
        ) -> *const c_void;
        fn CFStringCompare(left: *const c_void, right: *const c_void, flags: usize) -> isize;
        fn CFRelease(value: *const c_void);
    }
    struct StringRef(*const c_void);
    impl StringRef {
        fn new(name: &OsStr) -> io::Result<Self> {
            let bytes = name.as_bytes();
            let count = isize::try_from(bytes.len())
                .map_err(|_| io::Error::other("media output name too long"))?;
            // SAFETY: bytes live through the call; CF copies them with the default allocator.
            let value = unsafe {
                CFStringCreateWithBytes(std::ptr::null(), bytes.as_ptr(), count, 0x0800_0100, 0)
            };
            if value.is_null() {
                return Err(io::Error::other("media output name is not valid UTF-8"));
            }
            Ok(Self(value))
        }
    }
    impl Drop for StringRef {
        fn drop(&mut self) {
            // SAFETY: this owns one non-null create-rule CoreFoundation reference.
            unsafe { CFRelease(self.0) };
        }
    }
    pub(super) fn equivalent(left: &OsStr, right: &OsStr, insensitive: bool) -> io::Result<bool> {
        if left.as_bytes().is_ascii() && right.as_bytes().is_ascii() {
            return Ok(insensitive && left.as_bytes().eq_ignore_ascii_case(right.as_bytes()));
        }
        let (left, right) = (StringRef::new(left)?, StringRef::new(right)?);
        // kCFCompareNonliteral preserves canonical equivalence; case folding is
        // admitted only on a case-insensitive filesystem. This is conservative
        // collision refusal, not an exact reproduction of APFS's Unicode tables.
        let flags = 16 | usize::from(insensitive);
        // SAFETY: both owned references are valid CFStrings for this call.
        Ok(unsafe { CFStringCompare(left.0, right.0, flags) } == 0)
    }

    pub(super) fn case_alias(left: &Path, right: &Path) -> io::Result<bool> {
        use std::os::fd::AsRawFd;
        if left.parent() != right.parent()
            || !equivalent(left.file_name().unwrap(), right.file_name().unwrap(), true)?
        {
            return Ok(false);
        }
        let parent = fs::File::open(left.parent().unwrap())?;
        // SAFETY: the file owns this descriptor and fpathconf only queries it.
        let sensitive = unsafe { libc::fpathconf(parent.as_raw_fd(), libc::_PC_CASE_SENSITIVE) };
        if sensitive < 0 {
            return Err(io::Error::last_os_error());
        }
        equivalent(
            left.file_name().unwrap(),
            right.file_name().unwrap(),
            sensitive == 0,
        )
    }
}

#[cfg(test)]
mod tests;
