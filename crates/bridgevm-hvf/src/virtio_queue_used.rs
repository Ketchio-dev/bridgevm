//! Publish a used index only after both element fields were written.
use super::read_u16_at;
use crate::fwcfg::GuestMemoryMut;

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
    if mem.write_bytes(element, &u32::from(id).to_le_bytes())
        && mem.write_bytes(length, &len.to_le_bytes())
    {
        let _ = mem.write_bytes(index, &used.wrapping_add(1).to_le_bytes());
    }
}
