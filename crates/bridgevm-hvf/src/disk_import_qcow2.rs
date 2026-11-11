//! QCOW2 versions 2 and 3: the header and the active L1/L2 tables.
//! Refcounts, internal snapshots and header extensions are not needed to
//! read the current disk and are ignored, as `qemu-img convert` does.

use super::{be32, be64, within, Refusal, Result, Run, MAX_VIRTUAL_BYTES};
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
/// Only matters for compressed clusters, which are refused when met.
const INCOMPAT_COMPRESSION_TYPE: u64 = 1 << 3;
const INCOMPAT_EXTENDED_L2: u64 = 1 << 4;

pub(super) struct Qcow2 {
    pub(super) size: u64,
    cluster_bits: u32,
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
        if version == 3 {
            if readable < 104 || be32(&header, 100) < 104 {
                return Err(Refusal::Damaged("QCOW2 v3 header is cut short").into());
            }
            let incompatible = be64(&header, 72);
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
                    host: None,
                })?;
                continue;
            }
            if table % cluster != 0 || !within(table, cluster, self.file_len) {
                return Err(Refusal::Damaged("QCOW2 L2 table is outside the file").into());
            }
            file.read_exact_at(&mut l2, table)?;
            for slot in 0..span.div_ceil(cluster) {
                let entry = be64(&l2, slot as usize * 8);
                let virt = base + slot * cluster;
                let len = cluster.min(self.size - virt);
                if entry & L2_COMPRESSED != 0 {
                    return Err(Refusal::Compressed.into());
                }
                let data = entry & OFFSET_MASK;
                let host = if entry & L2_ZERO != 0 || data == 0 {
                    None
                } else if data % cluster != 0 || data >= self.file_len {
                    return Err(Refusal::Damaged("QCOW2 data cluster is outside the file").into());
                } else {
                    Some(data)
                };
                sink(Run { virt, len, host })?;
            }
        }
        Ok(())
    }
}
