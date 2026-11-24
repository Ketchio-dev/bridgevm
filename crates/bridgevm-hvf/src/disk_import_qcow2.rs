//! QCOW2 versions 2 and 3: the header and the active L1/L2 tables.
//! Refcounts, internal snapshots and header extensions are not needed to
//! read the current disk and are ignored, as `qemu-img convert` does.

use super::{be32, be64, read_padded, within, Refusal, Result, Run, RunSource, MAX_VIRTUAL_BYTES};
use std::fs::File;
use std::os::unix::fs::FileExt;

/// Host offset bits of an L1 or L2 entry (bits 9-55).
const OFFSET_MASK: u64 = 0x00ff_ffff_ffff_fe00;
const L2_COMPRESSED: u64 = 1 << 62;
const L2_ZERO: u64 = 1;
/// QEMU's own limit on the active L1 table (QCOW_MAX_L1_SIZE).
const MAX_L1_BYTES: u64 = 32 << 20;

const INCOMPAT_DIRTY: u64 = 1 << 0;
const INCOMPAT_CORRUPT: u64 = 1 << 1;
const INCOMPAT_DATA_FILE: u64 = 1 << 2;
/// A set bit selects a compression codec other than the default raw deflate.
const INCOMPAT_COMPRESSION_TYPE: u64 = 1 << 3;
const INCOMPAT_EXTENDED_L2: u64 = 1 << 4;

pub(super) struct Qcow2 {
    pub(super) size: u64,
    cluster_bits: u32,
    default_compression: bool,
    /// The L1 entries that cover the virtual size.
    l1: Vec<u64>,
    file_len: u64,
}

impl Qcow2 {
    pub(super) fn open(file: &File, file_len: u64) -> Result<Self> {
        let mut header = [0u8; 104];
        if file_len < 72 {
            return Err(Refusal::Damaged("QCOW2 header is cut short").into());
        }
        let readable = header.len().min(file_len as usize);
        file.read_exact_at(&mut header[..readable], 0)?;

        let version = be32(&header, 4);
        if version != 2 && version != 3 {
            return Err(Refusal::Unsupported(format!("QCOW2 version {version}")).into());
        }
        let cluster_bits = be32(&header, 20);
        if !(9..=21).contains(&cluster_bits) {
            return Err(Refusal::Damaged("QCOW2 cluster size out of range").into());
        }
        if be32(&header, 32) != 0 {
            return Err(Refusal::Encrypted.into());
        }
        if be64(&header, 8) != 0 {
            return Err(Refusal::BackingFile.into());
        }
        let mut default_compression = true;
        if version == 3 {
            if readable < 104 || be32(&header, 100) < 104 {
                return Err(Refusal::Damaged("QCOW2 v3 header is cut short").into());
            }
            let incompatible = be64(&header, 72);
            default_compression = incompatible & INCOMPAT_COMPRESSION_TYPE == 0;
            if incompatible & INCOMPAT_DATA_FILE != 0 {
                return Err(Refusal::ExternalDataFile.into());
            }
            if incompatible & INCOMPAT_EXTENDED_L2 != 0 {
                return Err(Refusal::ExtendedL2.into());
            }
            if incompatible & INCOMPAT_CORRUPT != 0 {
                return Err(Refusal::Corrupt.into());
            }
            if incompatible & INCOMPAT_DIRTY != 0 {
                return Err(Refusal::Dirty.into());
            }
            let known = INCOMPAT_DIRTY
                | INCOMPAT_CORRUPT
                | INCOMPAT_DATA_FILE
                | INCOMPAT_COMPRESSION_TYPE
                | INCOMPAT_EXTENDED_L2;
            if incompatible & !known != 0 {
                return Err(Refusal::Unsupported(format!(
                    "QCOW2 incompatible features {:#x}",
                    incompatible & !known
                ))
                .into());
            }
        }

        let size = be64(&header, 24);
        if size == 0 {
            return Err(Refusal::Damaged("QCOW2 virtual size is zero").into());
        }
        if size > MAX_VIRTUAL_BYTES {
            return Err(Refusal::TooLarge.into());
        }
        let cluster = 1u64 << cluster_bits;
        // One L2 table is one cluster of 8-byte entries.
        let per_l1 = cluster * (cluster / 8);
        let needed = size.div_ceil(per_l1);
        if needed * 8 > MAX_L1_BYTES {
            return Err(Refusal::Damaged("QCOW2 L1 table is too large").into());
        }
        if u64::from(be32(&header, 36)) < needed {
            return Err(Refusal::Damaged("QCOW2 L1 table is smaller than the disk").into());
        }
        let l1_offset = be64(&header, 40);
        if l1_offset == 0 || l1_offset % cluster != 0 {
            return Err(Refusal::Damaged("QCOW2 L1 table offset is invalid").into());
        }
        if !within(l1_offset, needed * 8, file_len) {
            return Err(Refusal::Damaged("QCOW2 L1 table is past the end of the file").into());
        }
        let mut raw = vec![0u8; (needed * 8) as usize];
        file.read_exact_at(&mut raw, l1_offset)?;
        let l1 = raw.chunks_exact(8).map(|e| be64(e, 0)).collect();
        Ok(Qcow2 {
            size,
            cluster_bits,
            default_compression,
            l1,
            file_len,
        })
    }

    pub(super) fn walk(&self, file: &File, sink: &mut dyn FnMut(Run) -> Result<()>) -> Result<()> {
        let cluster = 1u64 << self.cluster_bits;
        let per_l1 = cluster * (cluster / 8);
        let mut l2 = vec![0u8; cluster as usize];
        for (index, entry) in self.l1.iter().enumerate() {
            let base = index as u64 * per_l1;
            let span = per_l1.min(self.size - base);
            let table = entry & OFFSET_MASK;
            if table == 0 {
                sink(Run {
                    virt: base,
                    len: span,
                    source: RunSource::Hole,
                })?;
                continue;
            }
            if table % cluster != 0 || table >= self.file_len {
                return Err(Refusal::Damaged("QCOW2 L2 table is outside the file").into());
            }
            // Like a data cluster, a table the file ends inside reads as zeros past the end.
            read_padded(file, &mut l2, table)?;
            for slot in 0..span.div_ceil(cluster) {
                let entry = be64(&l2, slot as usize * 8);
                let virt = base + slot * cluster;
                let len = cluster.min(self.size - virt);
                let data = entry & OFFSET_MASK;
                let source = if entry & L2_COMPRESSED != 0 {
                    if !self.default_compression {
                        return Err(Refusal::Compressed.into());
                    }
                    RunSource::Deflate(CompressedCluster::from_entry(
                        entry,
                        self.cluster_bits,
                        self.file_len,
                    )?)
                } else if entry & L2_ZERO != 0 || data == 0 {
                    RunSource::Hole
                } else if data % cluster != 0 || data >= self.file_len {
                    return Err(Refusal::Damaged("QCOW2 data cluster is outside the file").into());
                } else {
                    RunSource::Data(data)
                };
                sink(Run { virt, len, source })?;
            }
        }
        Ok(())
    }
}

/// QCOW2 measures a compressed cluster in 512-byte sectors, starting at the
/// sector containing its first byte. Adjacent streams can share a sector.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct CompressedCluster {
    offset: u64,
    stored_bytes: usize,
    cluster_bytes: usize,
}

impl CompressedCluster {
    fn from_entry(entry: u64, cluster_bits: u32, file_len: u64) -> Result<Self> {
        let size_bits = cluster_bits - 8;
        let size_shift = 62 - size_bits;
        let offset = entry & ((1u64 << size_shift) - 1);
        if offset >= file_len || offset >> 56 != 0 {
            return Err(Refusal::Damaged("QCOW2 compressed cluster is outside the file").into());
        }
        let extra_sectors = (entry >> size_shift) & ((1 << size_bits) - 1);
        let bytes = (1 + extra_sectors) * 512 - offset % 512;
        // The file can end before the sector padding after the last stream.
        Ok(Self {
            offset,
            stored_bytes: bytes.min(file_len - offset) as usize,
            cluster_bytes: 1 << cluster_bits,
        })
    }

    pub(super) fn decode<'a>(&self, file: &File, buffer: &'a mut [u8]) -> Result<&'a [u8]> {
        // Even the largest descriptor occupies at most two 2 MiB clusters.
        // Both compressed bytes and a single output cluster fit the copy buffer.
        let (compressed, remaining) = buffer.split_at_mut(self.stored_bytes);
        file.read_exact_at(compressed, self.offset)?;
        let decoded = &mut remaining[..self.cluster_bytes];
        let result = miniz_oxide::inflate::decompress_slice_iter_to_slice(
            decoded,
            std::iter::once(&*compressed),
            false,
            true,
        );
        if result != Ok(self.cluster_bytes) {
            return Err(Refusal::Damaged("invalid QCOW2 deflate cluster").into());
        }
        Ok(decoded)
    }
}
