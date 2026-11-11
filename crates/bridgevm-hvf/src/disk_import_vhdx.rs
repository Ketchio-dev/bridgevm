//! VHDX (MS-VHDX v1): the headers, the region table, the metadata the BAT
//! needs, and the payload entries of the BAT. Differencing disks and a log
//! that still has to be replayed are refused; sector-bitmap entries only
//! matter for differencing disks and are skipped.

use super::{le16, le32, le64, within, Refusal, Result, Run, MAX_VIRTUAL_BYTES};
use std::fs::File;
use std::os::unix::fs::FileExt;

const KIB: u64 = 1024;
const MIB: u64 = 1024 * KIB;
const HEADER_OFFSETS: [u64; 2] = [64 * KIB, 128 * KIB];
const HEADER_BYTES: usize = 4096;
const REGION_OFFSETS: [u64; 2] = [192 * KIB, 256 * KIB];
const REGION_BYTES: usize = 64 * 1024;
const METADATA_TABLE_BYTES: usize = 64 * 1024;
/// The header section (identifier, headers, region tables) is the first MiB.
const HEADER_SECTION: u64 = MIB;
/// BAT entries read at a time while walking.
const BAT_BATCH: u64 = 64 * 1024;

// GUIDs in their on-disk (mixed-endian) byte order.
pub(super) const BAT_GUID: [u8; 16] = guid(0x2DC27766, 0xF623, 0x4200, 0x9D64115E9BFD4A08);
pub(super) const METADATA_GUID: [u8; 16] = guid(0x8B7CA206, 0x4790, 0x4B9A, 0xB8FE575F050F886E);
pub(super) const FILE_PARAMETERS_GUID: [u8; 16] =
    guid(0xCAA16737, 0xFA36, 0x4D43, 0xB3B633F0AA44E76B);
pub(super) const VIRTUAL_DISK_SIZE_GUID: [u8; 16] =
    guid(0x2FA54224, 0xCD1B, 0x4876, 0xB2115DBED83BF4B8);
pub(super) const LOGICAL_SECTOR_SIZE_GUID: [u8; 16] =
    guid(0x8141BF1D, 0xA96F, 0x4709, 0xBA47F233A8FAAB5F);
const PHYSICAL_SECTOR_SIZE_GUID: [u8; 16] = guid(0xCDA348C7, 0x445D, 0x4471, 0x9CC9E9885251C556);
const VIRTUAL_DISK_ID_GUID: [u8; 16] = guid(0xBECA12AB, 0xB2E6, 0x4523, 0x93EFC309E000C746);
const PARENT_LOCATOR_GUID: [u8; 16] = guid(0xA8D35F2D, 0xB30B, 0x454D, 0xABF7D3D84834AB0C);

const fn guid(a: u32, b: u16, c: u16, d: u64) -> [u8; 16] {
    let (a, b, c, d) = (
        a.to_le_bytes(),
        b.to_le_bytes(),
        c.to_le_bytes(),
        d.to_be_bytes(),
    );
    [
        a[0], a[1], a[2], a[3], b[0], b[1], c[0], c[1], d[0], d[1], d[2], d[3], d[4], d[5], d[6],
        d[7],
    ]
}

/// CRC-32C (Castagnoli), the VHDX checksum.
pub(super) fn crc32c(bytes: &[u8]) -> u32 {
    const TABLE: [u32; 256] = {
        let mut table = [0u32; 256];
        let mut i = 0;
        while i < 256 {
            let mut crc = i as u32;
            let mut bit = 0;
            while bit < 8 {
                crc = if crc & 1 != 0 {
                    (crc >> 1) ^ 0x82F6_3B78
                } else {
                    crc >> 1
                };
                bit += 1;
            }
            table[i] = crc;
            i += 1;
        }
        table
    };
    !bytes.iter().fold(!0u32, |crc, &b| {
        TABLE[((crc ^ u32::from(b)) & 0xff) as usize] ^ (crc >> 8)
    })
}

/// A structure whose CRC-32C at `at` covers all of it with that field zeroed.
fn checksum_ok(bytes: &mut [u8], at: usize) -> bool {
    let stored = le32(bytes, at);
    bytes[at..at + 4].fill(0);
    crc32c(bytes) == stored
}

pub(super) struct Vhdx {
    pub(super) size: u64,
    block: u64,
    /// Payload blocks per sector-bitmap block: the BAT interleaves one
    /// bitmap entry after every `chunk_ratio` payload entries.
    chunk_ratio: u64,
    bat_offset: u64,
    file_len: u64,
}

impl Vhdx {
    pub(super) fn open(file: &File, file_len: u64) -> Result<Self> {
        if file_len < HEADER_SECTION {
            return Err(Refusal::Damaged("VHDX header section is cut short").into());
        }
        // The current header is the valid one with the higher sequence number.
        let mut current: Option<(u64, Vec<u8>)> = None;
        for offset in HEADER_OFFSETS {
            let mut header = vec![0u8; HEADER_BYTES];
            file.read_exact_at(&mut header, offset)?;
            if &header[..4] != b"head" || !checksum_ok(&mut header, 4) {
                continue;
            }
            let sequence = le64(&header, 8);
            if current.as_ref().is_none_or(|(best, _)| sequence > *best) {
                current = Some((sequence, header));
            }
        }
        let Some((_, header)) = current else {
            return Err(Refusal::Damaged("no valid VHDX header").into());
        };
        if le16(&header, 66) != 1 {
            return Err(Refusal::Unsupported(format!("VHDX version {}", le16(&header, 66))).into());
        }
        // A non-zero log GUID means the log may hold writes not yet applied.
        if header[48..64].iter().any(|&b| b != 0) {
            return Err(Refusal::VhdxLog.into());
        }

        let (bat, metadata) = regions(file, file_len)?;
        let params = Metadata::read(file, metadata)?;
        if params.has_parent {
            return Err(Refusal::Differencing.into());
        }
        let block = params.block;
        let chunk_ratio = (1u64 << 23) * params.logical_sector / block;
        let payload = params.size.div_ceil(block);
        let entries = payload + (payload - 1) / chunk_ratio;
        if bat.1 < entries * 8 {
            return Err(Refusal::Damaged("VHDX BAT is smaller than the disk").into());
        }
        Ok(Vhdx {
            size: params.size,
            block,
            chunk_ratio,
            bat_offset: bat.0,
            file_len,
        })
    }

    pub(super) fn walk(&self, file: &File, sink: &mut dyn FnMut(Run) -> Result<()>) -> Result<()> {
        let payload = self.size.div_ceil(self.block);
        let entries = payload + (payload - 1) / self.chunk_ratio;
        let mut raw = vec![0u8; (BAT_BATCH.min(entries) * 8) as usize];
        let mut block_index = 0u64;
        let mut first = 0u64;
        while first < entries {
            let count = BAT_BATCH.min(entries - first);
            let raw = &mut raw[..(count * 8) as usize];
            file.read_exact_at(raw, self.bat_offset + first * 8)?;
            for (i, entry) in raw.chunks_exact(8).map(|e| le64(e, 0)).enumerate() {
                if (first + i as u64) % (self.chunk_ratio + 1) == self.chunk_ratio {
                    continue; // a sector-bitmap entry
                }
                let virt = block_index * self.block;
                let len = self.block.min(self.size - virt);
                let host = match entry & 7 {
                    // NOT_PRESENT, UNDEFINED, ZERO, UNMAPPED
                    0..=3 => None,
                    6 => {
                        let offset = entry & !(MIB - 1);
                        if offset < HEADER_SECTION || offset >= self.file_len {
                            return Err(Refusal::Damaged("VHDX block is outside the file").into());
                        }
                        Some(offset)
                    }
                    // PARTIALLY_PRESENT: only a differencing disk has these.
                    7 => return Err(Refusal::Differencing.into()),
                    _ => return Err(Refusal::Damaged("VHDX block state is unknown").into()),
                };
                sink(Run { virt, len, host })?;
                block_index += 1;
            }
            first += count;
        }
        debug_assert_eq!(block_index, payload);
        Ok(())
    }
}

/// The BAT and metadata regions as (offset, length), from the first valid
/// region table.
fn regions(file: &File, file_len: u64) -> Result<((u64, u64), (u64, u64))> {
    for offset in REGION_OFFSETS {
        let mut table = vec![0u8; REGION_BYTES];
        file.read_exact_at(&mut table, offset)?;
        if &table[..4] != b"regi" || !checksum_ok(&mut table, 4) {
            continue;
        }
        let count = le32(&table, 8) as usize;
        if count > 2047 {
            return Err(Refusal::Damaged("VHDX region table is too long").into());
        }
        let (mut bat, mut metadata) = (None, None);
        for entry in table[16..16 + count * 32].chunks_exact(32) {
            let region = (le64(entry, 16), u64::from(le32(entry, 24)));
            let slot = if entry[..16] == BAT_GUID {
                &mut bat
            } else if entry[..16] == METADATA_GUID {
                &mut metadata
            } else if le32(entry, 28) & 1 != 0 {
                return Err(Refusal::Unsupported("a required VHDX region".into()).into());
            } else {
                continue;
            };
            if region.0 < HEADER_SECTION || region.1 == 0 || !within(region.0, region.1, file_len) {
                return Err(Refusal::Damaged("a VHDX region is outside the file").into());
            }
            *slot = Some(region);
        }
        return match (bat, metadata) {
            (Some(bat), Some(metadata)) => Ok((bat, metadata)),
            _ => Err(Refusal::Damaged("VHDX BAT or metadata region is missing").into()),
        };
    }
    Err(Refusal::Damaged("no valid VHDX region table").into())
}

struct Metadata {
    block: u64,
    has_parent: bool,
    size: u64,
    logical_sector: u64,
}

impl Metadata {
    fn read(file: &File, (offset, len): (u64, u64)) -> Result<Self> {
        if len < METADATA_TABLE_BYTES as u64 {
            return Err(Refusal::Damaged("VHDX metadata region is too small").into());
        }
        let mut table = vec![0u8; METADATA_TABLE_BYTES];
        file.read_exact_at(&mut table, offset)?;
        if &table[..8] != b"metadata" {
            return Err(Refusal::Damaged("VHDX metadata table signature").into());
        }
        let count = usize::from(le16(&table, 10));
        if count > 2047 {
            return Err(Refusal::Damaged("VHDX metadata table is too long").into());
        }
        let (mut params, mut size, mut sector) = (None, None, None);
        for entry in table[32..32 + count * 32].chunks_exact(32) {
            let id: [u8; 16] = entry[..16].try_into().unwrap();
            let (at, item_len) = (u64::from(le32(entry, 16)), u64::from(le32(entry, 20)));
            let wanted = match id {
                FILE_PARAMETERS_GUID => 8,
                VIRTUAL_DISK_SIZE_GUID => 8,
                LOGICAL_SECTOR_SIZE_GUID => 4,
                PHYSICAL_SECTOR_SIZE_GUID | VIRTUAL_DISK_ID_GUID | PARENT_LOCATOR_GUID => continue,
                _ if le32(entry, 24) & 4 != 0 => {
                    return Err(Refusal::Unsupported("a required VHDX metadata item".into()).into())
                }
                _ => continue,
            };
            if item_len < wanted || !within(at, item_len, len) || at < METADATA_TABLE_BYTES as u64 {
                return Err(Refusal::Damaged("a VHDX metadata item is outside its region").into());
            }
            let mut value = [0u8; 8];
            file.read_exact_at(&mut value[..wanted as usize], offset + at)?;
            match id {
                FILE_PARAMETERS_GUID => {
                    params = Some((u64::from(le32(&value, 0)), le32(&value, 4) & 2 != 0))
                }
                VIRTUAL_DISK_SIZE_GUID => size = Some(le64(&value, 0)),
                _ => sector = Some(u64::from(le32(&value, 0))),
            }
        }
        let (Some((block, has_parent)), Some(size), Some(logical_sector)) = (params, size, sector)
        else {
            return Err(Refusal::Damaged("VHDX metadata is incomplete").into());
        };
        if has_parent {
            return Ok(Metadata {
                block,
                has_parent,
                size,
                logical_sector,
            });
        }
        if !block.is_power_of_two() || !(MIB..=256 * MIB).contains(&block) {
            return Err(Refusal::Damaged("VHDX block size is out of range").into());
        }
        if logical_sector != 512 && logical_sector != 4096 {
            return Err(Refusal::Damaged("VHDX logical sector size is invalid").into());
        }
        if size == 0 {
            return Err(Refusal::Damaged("VHDX virtual size is zero").into());
        }
        if size > MAX_VIRTUAL_BYTES {
            return Err(Refusal::TooLarge.into());
        }
        Ok(Metadata {
            block,
            has_parent,
            size,
            logical_sector,
        })
    }
}
