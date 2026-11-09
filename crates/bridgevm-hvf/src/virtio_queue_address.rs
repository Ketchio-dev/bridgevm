//! Checked offsets for guest-configured split-queue bases.
use crate::fwcfg::GuestMemoryMut;
use std::sync::atomic::{fence, Ordering};
pub(crate) const DESC_SIZE: u64 = 16;
/// The driver's `avail.idx`. The ring entries and descriptors it publishes are
/// read after it, and Arm may otherwise read them first: a new index with a
/// stale entry (QEMU's virtqueue_num_heads: smp_rmb after the index).
pub(crate) fn read_avail_idx(mem: &dyn GuestMemoryMut, driver: u64) -> Option<u16> {
    let idx = read_u16_at(mem, driver, 2)?;
    fence(Ordering::Acquire);
    Some(idx)
}
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
