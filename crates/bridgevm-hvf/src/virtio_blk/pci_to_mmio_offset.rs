//! Split out of virtio_blk.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::fs::File;
use std::io;
use std::io::Read;
use std::io::Seek;
use std::io::SeekFrom;
use std::path::Path;

pub(crate) fn pci_to_mmio_offset(offset: u64, is_write: bool) -> Option<u64> {
    if (PCI_COMMON_CFG_OFFSET..PCI_COMMON_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(offset - PCI_COMMON_CFG_OFFSET);
    }
    if (PCI_DEVICE_CFG_OFFSET..PCI_DEVICE_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_CONFIG + offset - PCI_DEVICE_CFG_OFFSET);
    }
    if (PCI_NOTIFY_CFG_OFFSET..PCI_NOTIFY_CFG_OFFSET + PCI_CFG_REGION_SIZE).contains(&offset) {
        return Some(REG_QUEUE_NOTIFY);
    }
    if offset == PCI_ISR_CFG_OFFSET {
        return Some(if is_write {
            REG_INTERRUPT_ACK
        } else {
            REG_INTERRUPT_STATUS
        });
    }
    None
}

#[derive(Debug)]
pub(crate) struct RawFileBackend {
    pub(crate) file: File,
    pub(crate) len: u64,
}

impl RawFileBackend {
    pub(crate) fn open(path: impl AsRef<Path>) -> io::Result<Self> {
        let file = File::open(path)?;
        let len = file.metadata()?.len();
        Ok(Self { file, len })
    }

    pub(crate) fn capacity_sectors(&self) -> u64 {
        self.len.div_ceil(SECTOR_SIZE)
    }

    pub(crate) fn read_at_into(&mut self, byte_offset: u64, dst: &mut [u8]) -> io::Result<()> {
        let end = byte_offset
            .checked_add(dst.len() as u64)
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "read offset overflow"))?;
        if end > self.capacity_sectors() * SECTOR_SIZE {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "virtio block read past media",
            ));
        }
        if byte_offset >= self.len {
            dst.fill(0);
            return Ok(());
        }
        let readable = (self.len - byte_offset).min(dst.len() as u64) as usize;
        self.file.seek(SeekFrom::Start(byte_offset))?;
        self.file.read_exact(&mut dst[..readable])?;
        if readable < dst.len() {
            dst[readable..].fill(0);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; 16];
        if !mem.read_into(gpa, &mut bytes) {
            return None;
        }
        Some(Self {
            addr: u64::from_le_bytes(bytes[0..8].try_into().unwrap()),
            len: u32::from_le_bytes(bytes[8..12].try_into().unwrap()),
            flags: u16::from_le_bytes(bytes[12..14].try_into().unwrap()),
            next: u16::from_le_bytes(bytes[14..16].try_into().unwrap()),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub(crate) struct RequestCompletion {
    pub(crate) written_len: u32,
}

impl RequestCompletion {
    pub(crate) fn status_only(mem: &mut dyn GuestMemoryMut, status_addr: u64, status: u8) -> Self {
        if status_addr != 0 {
            let _ = mem.write_bytes(status_addr, &[status]);
        }
        Self { written_len: 1 }
    }

    pub(crate) fn write_status(
        mem: &mut dyn GuestMemoryMut,
        status_desc: Option<&Descriptor>,
        status: u8,
        written_len: u32,
    ) -> Self {
        if let Some(desc) = status_desc {
            if desc.flags & DESC_F_WRITE != 0 && desc.len >= 1 {
                let _ = mem.write_bytes(desc.addr, &[status]);
            }
        }
        Self { written_len }
    }
}

pub(crate) fn set_low(current: u64, value: u64) -> u64 {
    (current & !0xffff_ffff) | (value & 0xffff_ffff)
}

pub(crate) fn set_high(current: u64, value: u64) -> u64 {
    (current & 0xffff_ffff) | ((value & 0xffff_ffff) << 32)
}

pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn align_up(value: u64, align: u64) -> u64 {
    let align = align.max(1);
    value.div_ceil(align).saturating_mul(align)
}

pub(crate) fn read_le_from_bytes(bytes: &[u8], offset: u64, size: u8) -> Option<u64> {
    let offset = usize::try_from(offset).ok()?;
    let size = usize::from(size);
    if offset.checked_add(size)? > bytes.len() || size > 8 {
        return None;
    }
    let mut buf = [0u8; 8];
    buf[..size].copy_from_slice(&bytes[offset..offset + size]);
    Some(u64::from_le_bytes(buf))
}

pub(crate) fn read_u16(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u16> {
    let mut bytes = [0u8; 2];
    if !mem.read_into(gpa, &mut bytes) {
        return None;
    }
    Some(u16::from_le_bytes(bytes))
}
