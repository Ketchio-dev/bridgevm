//! Checked offsets for guest-configured split-queue bases.
use crate::fwcfg::GuestMemoryMut;
pub(crate) const DESC_SIZE: u64 = 16;
pub(crate) fn read_u16_at(mem: &dyn GuestMemoryMut, base: u64, offset: u64) -> Option<u16> {
    let address = base.checked_add(offset)?;
    address.checked_add(2)?;
    let mut bytes = [0; 2];
    mem.read_into(address, &mut bytes)
        .then(|| u16::from_le_bytes(bytes))
}
pub(crate) fn read_descriptor<T>(
    mem: &dyn GuestMemoryMut,
    base: u64,
    index: u16,
    read: impl FnOnce(&dyn GuestMemoryMut, u64) -> Option<T>,
) -> Option<T> {
    let address = base.checked_add(u64::from(index) * DESC_SIZE)?;
    address.checked_add(DESC_SIZE)?;
    read(mem, address)
}
pub(crate) fn write_used(mem: &mut dyn GuestMemoryMut, base: u64, size: u16, id: u16, len: u32) {
    if size == 0 || base == 0 {
        return;
    }
    let Some(index) = base.checked_add(2) else {
        return;
    };
    let Some(used) = read_u16_at(mem, base, 2) else {
        return;
    };
    let Some(element) = base.checked_add(4 + u64::from(used % size) * 8) else {
        return;
    };
    let Some(length) = element.checked_add(4) else {
        return;
    };
    if element.checked_add(8).is_none() {
        return;
    }
    let _ = mem.write_bytes(element, &u32::from(id).to_le_bytes());
    let _ = mem.write_bytes(length, &len.to_le_bytes());
    let _ = mem.write_bytes(index, &used.wrapping_add(1).to_le_bytes());
}
