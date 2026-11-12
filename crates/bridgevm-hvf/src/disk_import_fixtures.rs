//! QCOW2 and VHDX images built byte by byte for the converter's tests, so
//! the suite needs no qemu-img. Each builder also returns the raw disk the
//! image describes.

use super::vhdx::crc32c;
use std::path::PathBuf;

pub(super) struct Scratch(pub(super) PathBuf);

impl Scratch {
    pub(super) fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bv-diskimport-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Scratch(dir)
    }
    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    pub(super) fn write(&self, name: &str, bytes: &[u8]) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, bytes).unwrap();
        path
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Deterministic, non-zero-heavy test data.
pub(super) fn pattern(len: usize, seed: u64) -> Vec<u8> {
    let mut x = seed | 1;
    (0..len)
        .map(|_| {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            (x >> 24) as u8 | 1
        })
        .collect()
}

pub(super) enum Cluster {
    Data(Vec<u8>),
    /// v3 zero flag without an allocation.
    Zero,
    /// Zero flag on an allocated cluster whose stale bytes must not appear.
    ZeroAllocated(Vec<u8>),
    Compressed,
}

/// Header field offsets tests patch.
pub(super) mod qcow2_field {
    pub(crate) const BACKING_OFFSET: usize = 8;
    pub(crate) const CLUSTER_BITS: usize = 20;
    pub(crate) const SIZE: usize = 24;
    pub(crate) const CRYPT: usize = 32;
    pub(crate) const L1_SIZE: usize = 36;
    pub(crate) const L1_OFFSET: usize = 40;
    pub(crate) const INCOMPATIBLE: usize = 72;
}

/// A QCOW2 image: header in cluster 0, the L1 table from cluster 1, then L2
/// tables, then data clusters in the order given (not virtual order).
pub(super) fn qcow2(
    version: u32,
    cluster_bits: u32,
    size: u64,
    clusters: &[(u64, Cluster)],
) -> (Vec<u8>, Vec<u8>) {
    let cluster = 1usize << cluster_bits;
    let per_l2 = cluster / 8;
    let l1_entries = (size as usize).div_ceil(cluster * per_l2);
    let l1_clusters = (l1_entries * 8).div_ceil(cluster);
    let mut next = 1 + l1_clusters;
    let mut l2_of = vec![None; l1_entries];
    for (index, _) in clusters {
        let l1 = *index as usize / per_l2;
        if l2_of[l1].is_none() {
            l2_of[l1] = Some(next);
            next += 1;
        }
    }
    let mut file = vec![0u8; next * cluster];
    let mut raw = vec![0u8; size as usize];
    for (index, kind) in clusters {
        let index = *index as usize;
        let slot = l2_of[index / per_l2].unwrap() * cluster + (index % per_l2) * 8;
        let mut place = |bytes: &[u8]| {
            let at = file.len();
            file.extend_from_slice(bytes);
            file.resize(at + cluster, 0);
            at as u64
        };
        let entry = match kind {
            Cluster::Data(bytes) => {
                let virt = index * cluster;
                let end = (virt + cluster).min(raw.len());
                raw[virt..end].copy_from_slice(&bytes[..end - virt]);
                place(bytes) | 1 << 63
            }
            Cluster::Zero => 1,
            Cluster::ZeroAllocated(bytes) => place(bytes) | 1,
            Cluster::Compressed => 1 << 62 | (1 + l1_clusters as u64) << cluster_bits,
        };
        file[slot..slot + 8].copy_from_slice(&entry.to_be_bytes());
    }
    for (l1, l2) in l2_of.iter().enumerate() {
        if let Some(l2) = l2 {
            let entry = ((l2 * cluster) as u64 | 1 << 63).to_be_bytes();
            file[cluster + l1 * 8..cluster + l1 * 8 + 8].copy_from_slice(&entry);
        }
    }
    file[..4].copy_from_slice(b"QFI\xfb");
    file[4..8].copy_from_slice(&version.to_be_bytes());
    file[20..24].copy_from_slice(&cluster_bits.to_be_bytes());
    file[24..32].copy_from_slice(&size.to_be_bytes());
    file[36..40].copy_from_slice(&(l1_entries as u32).to_be_bytes());
    file[40..48].copy_from_slice(&(cluster as u64).to_be_bytes());
    if version == 3 {
        file[96..100].copy_from_slice(&4u32.to_be_bytes());
        file[100..104].copy_from_slice(&104u32.to_be_bytes());
    }
    (file, raw)
}

pub(super) fn patch_be(file: &mut [u8], at: usize, value: u64, width: usize) {
    file[at..at + width].copy_from_slice(&value.to_be_bytes()[8 - width..]);
}

pub(super) enum Block {
    Data(Vec<u8>),
    /// A BAT state other than FULLY_PRESENT: 0-3 are holes, 7 is partial.
    State(u64),
}

pub(super) const MIB: usize = 1 << 20;
/// Where the builder puts its structures.
pub(super) const VHDX_METADATA: usize = MIB;
pub(super) const VHDX_BAT: usize = 2 * MIB;
pub(super) const VHDX_HEADERS: [usize; 2] = [64 << 10, 128 << 10];
pub(super) const VHDX_REGIONS: [usize; 2] = [192 << 10, 256 << 10];

pub(super) struct VhdxSpec {
    pub(super) block: usize,
    pub(super) logical_sector: u32,
    pub(super) size: u64,
    pub(super) has_parent: bool,
    pub(super) log_guid: [u8; 16],
}

impl VhdxSpec {
    pub(super) fn new(block: usize, size: u64) -> Self {
        VhdxSpec {
            block,
            logical_sector: 512,
            size,
            has_parent: false,
            log_guid: [0; 16],
        }
    }
}

/// A VHDX image: header section, metadata at 1 MiB, the BAT at 2 MiB, then
/// payload blocks in the order given. The raw disk is only built up to 64 MiB.
pub(super) fn vhdx(spec: &VhdxSpec, blocks: &[(u64, Block)]) -> (Vec<u8>, Vec<u8>) {
    let block = spec.block;
    let chunk_ratio = ((1usize << 23) * spec.logical_sector as usize) / block;
    let payload = (spec.size as usize).div_ceil(block);
    let entries = payload + (payload - 1) / chunk_ratio;
    let bat_len = (entries * 8).div_ceil(MIB) * MIB;
    let mut file = vec![0u8; VHDX_BAT + bat_len];
    let mut raw = vec![
        0u8;
        if spec.size <= 64 << 20 {
            spec.size as usize
        } else {
            0
        }
    ];
    for (index, kind) in blocks {
        let index = *index as usize;
        let slot = VHDX_BAT + (index + index / chunk_ratio) * 8;
        let entry = match kind {
            Block::Data(bytes) => {
                let at = file.len();
                file.extend_from_slice(bytes);
                file.resize(at + block, 0);
                let virt = index * block;
                if virt < raw.len() {
                    let end = (virt + block).min(raw.len());
                    raw[virt..end].copy_from_slice(&bytes[..end - virt]);
                }
                at as u64 | 6
            }
            Block::State(state) => *state,
        };
        file[slot..slot + 8].copy_from_slice(&entry.to_le_bytes());
    }
    file[..8].copy_from_slice(b"vhdxfile");
    for (sequence, at) in VHDX_HEADERS.into_iter().enumerate() {
        let header = &mut file[at..at + 4096];
        header[..4].copy_from_slice(b"head");
        header[8..16].copy_from_slice(&(sequence as u64 + 1).to_le_bytes());
        header[48..64].copy_from_slice(&spec.log_guid);
        header[66..68].copy_from_slice(&1u16.to_le_bytes());
        header[68..72].copy_from_slice(&(MIB as u32).to_le_bytes());
        header[72..80].copy_from_slice(&(MIB as u64 * 3).to_le_bytes());
        seal(header);
    }
    for at in VHDX_REGIONS {
        let table = &mut file[at..at + (64 << 10)];
        table[..4].copy_from_slice(b"regi");
        table[8..12].copy_from_slice(&2u32.to_le_bytes());
        region(&mut table[16..48], super::vhdx::BAT_GUID, VHDX_BAT, bat_len);
        region(
            &mut table[48..80],
            super::vhdx::METADATA_GUID,
            VHDX_METADATA,
            MIB,
        );
        seal(table);
    }
    let meta = &mut file[VHDX_METADATA..VHDX_METADATA + MIB];
    meta[..8].copy_from_slice(b"metadata");
    let flags = u32::from(spec.has_parent) << 1;
    let items: [([u8; 16], Vec<u8>); 3] = [
        (
            super::vhdx::FILE_PARAMETERS_GUID,
            [(block as u32).to_le_bytes(), flags.to_le_bytes()].concat(),
        ),
        (
            super::vhdx::VIRTUAL_DISK_SIZE_GUID,
            spec.size.to_le_bytes().to_vec(),
        ),
        (
            super::vhdx::LOGICAL_SECTOR_SIZE_GUID,
            spec.logical_sector.to_le_bytes().to_vec(),
        ),
    ];
    meta[10..12].copy_from_slice(&(items.len() as u16).to_le_bytes());
    for (i, (id, value)) in items.iter().enumerate() {
        let at = (64 << 10) + i * 64;
        let entry = &mut meta[32 + i * 32..64 + i * 32];
        entry[..16].copy_from_slice(id);
        entry[16..20].copy_from_slice(&(at as u32).to_le_bytes());
        entry[20..24].copy_from_slice(&(value.len() as u32).to_le_bytes());
        entry[24..28].copy_from_slice(&6u32.to_le_bytes()); // virtual disk, required
        meta[at..at + value.len()].copy_from_slice(value);
    }
    (file, raw)
}

fn region(entry: &mut [u8], id: [u8; 16], offset: usize, len: usize) {
    entry[..16].copy_from_slice(&id);
    entry[16..24].copy_from_slice(&(offset as u64).to_le_bytes());
    entry[24..28].copy_from_slice(&(len as u32).to_le_bytes());
    entry[28..32].copy_from_slice(&1u32.to_le_bytes());
}

/// Store the CRC-32C of a VHDX header or region table at byte 4.
pub(super) fn seal(structure: &mut [u8]) {
    structure[4..8].fill(0);
    let crc = crc32c(structure);
    structure[4..8].copy_from_slice(&crc.to_le_bytes());
}
