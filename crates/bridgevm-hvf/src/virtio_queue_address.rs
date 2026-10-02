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
#[path = "virtio_queue_used.rs"]
mod used;
pub(crate) use used::write_used;
