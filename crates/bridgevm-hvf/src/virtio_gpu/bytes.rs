//! Little-endian scalar readers over guest memory and request/response buffers.

use crate::fwcfg::GuestMemoryMut;

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
    mem.read_into(gpa, &mut bytes)
        .then(|| u16::from_le_bytes(bytes))
}

pub(crate) fn read_le_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    Some(u32::from_le_bytes(
        bytes.get(offset..offset + 4)?.try_into().ok()?,
    ))
}

pub(crate) fn read_le_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    Some(u64::from_le_bytes(
        bytes.get(offset..offset + 8)?.try_into().ok()?,
    ))
}
