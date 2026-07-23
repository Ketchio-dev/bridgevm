//! Namespace identity (NSID/NGUID/EUI64/UUID) and per-namespace backend routing, sizing, and media surface.

use super::*;
use std::io;
use std::path::Path;

/// The primary namespace's identifier (NSID 1).
pub const NSID: u32 = 1;

/// Optional second namespace (NSID 2), used as a blank Windows install target
/// alongside the NSID-1 installer source.
pub const NSID2: u32 = 2;

pub(crate) const NS_EUI64: [u8; 8] = *b"BVMNVME1";

pub(crate) const NS_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS1";

pub(crate) const NS_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
];

pub(crate) const NS2_EUI64: [u8; 8] = *b"BVMNVME2";

pub(crate) const NS2_NGUID: [u8; 16] = *b"BridgeVM-NVMeNS2";

pub(crate) const NS2_UUID: [u8; 16] = [
    0x42, 0x56, 0x4d, 0x00, 0x20, 0x26, 0x06, 0x20, 0x80, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x02,
];

/// Per-namespace stable identifiers (NGUID, EUI64, UUID) for NSID 1 and 2.
pub(crate) fn namespace_identifiers(nsid: u32) -> ([u8; 16], [u8; 8], [u8; 16]) {
    if nsid == NSID2 {
        (NS2_NGUID, NS2_EUI64, NS2_UUID)
    } else {
        (NS_NGUID, NS_EUI64, NS_UUID)
    }
}

pub(crate) fn second_namespace_missing() -> io::Error {
    io::Error::new(io::ErrorKind::NotFound, "NVMe NSID 2 is not attached")
}

impl NvmeController {
    /// Attach a blank NSID-2 target namespace backed by `disk_bytes` of in-memory
    /// storage (rounded to a whole LBA). Windows sees a second, empty disk.
    pub fn attach_second_namespace(&mut self, disk_bytes: usize) {
        self.disk2 = Some(DiskBackend::memory(vec![0u8; rounded_disk_len(disk_bytes)]));
    }

    /// Attach an NSID-2 target namespace backed by a host raw file (see
    /// [`Self::with_raw_file`] for the `write_back` semantics).
    pub fn attach_second_namespace_raw_file(
        &mut self,
        path: impl AsRef<Path>,
        write_back: bool,
    ) -> io::Result<()> {
        self.disk2 = Some(DiskBackend::raw_file(path, write_back)?);
        Ok(())
    }

    /// Whether an NSID-2 target namespace is attached.
    pub fn has_second_namespace(&self) -> bool {
        self.disk2.is_some()
    }

    /// The number of active namespaces (1 or 2).
    pub(crate) fn namespace_count(&self) -> u32 {
        if self.disk2.is_some() {
            2
        } else {
            1
        }
    }

    /// Immutable backing store for `nsid`, if that namespace is active.
    pub(crate) fn backend_for_nsid(&self, nsid: u32) -> Option<&DiskBackend> {
        match nsid {
            NSID => Some(&self.disk),
            NSID2 => self.disk2.as_ref(),
            _ => None,
        }
    }

    /// Mutable backing store for `nsid`, if that namespace is active.
    pub(crate) fn backend_for_nsid_mut(&mut self, nsid: u32) -> Option<&mut DiskBackend> {
        match nsid {
            NSID => Some(&mut self.disk),
            NSID2 => self.disk2.as_mut(),
            _ => None,
        }
    }

    /// Block count for a specific namespace.
    pub(crate) fn block_count_for(&self, nsid: u32) -> u64 {
        self.backend_for_nsid(nsid)
            .map(|b| b.byte_len() / LBA_SIZE as u64)
            .unwrap_or(0)
    }

    /// Snapshot of the current disk image, including guest writes that have been
    /// processed through the NVMe queues. This is only available for the small
    /// in-memory backend used by unit tests and ad-hoc media.
    pub fn disk_image(&self) -> &[u8] {
        self.disk
            .memory_image()
            .expect("NVMe disk image is host-file backed; export it instead")
    }

    /// Snapshot view for callers that need to distinguish memory-backed media
    /// from large host-file-backed media.
    pub fn disk_image_if_memory(&self) -> Option<&[u8]> {
        self.disk.memory_image()
    }

    pub fn second_namespace_disk_image_if_memory(&self) -> Option<&[u8]> {
        self.disk2.as_ref().and_then(DiskBackend::memory_image)
    }

    /// Export the full current disk image to `path`, applying any sparse overlay
    /// writes on top of the source raw file.
    pub fn export_disk_image(&mut self, path: impl AsRef<Path>) -> io::Result<u64> {
        self.disk.export_to_path(path)
    }

    pub fn export_second_namespace_disk_image(
        &mut self,
        path: impl AsRef<Path>,
    ) -> io::Result<u64> {
        self.disk2
            .as_mut()
            .ok_or_else(second_namespace_missing)?
            .export_to_path(path)
    }

    /// Flush host-file-backed write-through media.
    pub fn flush_disk(&mut self) -> io::Result<()> {
        self.disk.flush()
    }

    pub fn flush_second_namespace_disk(&mut self) -> io::Result<()> {
        self.disk2
            .as_mut()
            .ok_or_else(second_namespace_missing)?
            .flush()
    }

    /// Current byte length of the backing disk.
    pub fn disk_len(&self) -> u64 {
        self.disk.byte_len()
    }

    pub fn second_namespace_disk_len(&self) -> Option<u64> {
        self.disk2.as_ref().map(DiskBackend::byte_len)
    }

    /// Number of `LBA_SIZE`-byte logical blocks in the backing disk.
    pub fn block_count(&self) -> u64 {
        self.disk.byte_len() / LBA_SIZE as u64
    }
}
