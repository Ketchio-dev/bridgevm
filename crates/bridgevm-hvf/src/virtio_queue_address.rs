//! Checked offsets for guest-configured split-queue bases.
use crate::fwcfg::GuestMemoryMut;
use std::sync::atomic::{fence, Ordering};
pub(crate) const DESC_SIZE: u64 = 16;
pub(crate) const DESC_F_NEXT: u16 = 1;
pub(crate) const DESC_F_WRITE: u16 = 2;
/// One split-ring descriptor as the guest wrote it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Descriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}
impl Descriptor {
    pub(crate) fn read(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<Self> {
        let mut bytes = [0u8; DESC_SIZE as usize];
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
/// The chain starting at `head` in the `size`-entry table at `desc`. False,
/// with `out` empty, for a head or link outside the table, an unreadable
/// descriptor, or a chain longer than the table.
pub(crate) fn descriptor_chain_into(
    mem: &dyn GuestMemoryMut,
    desc: u64,
    size: u16,
    head: u16,
    out: &mut Vec<Descriptor>,
) -> bool {
    out.clear();
    if head >= size {
        return false;
    }
    let mut index = head;
    for _ in 0..size {
        let Some(descriptor) = read_descriptor(mem, desc, index, Descriptor::read) else {
            out.clear();
            return false;
        };
        let has_next = descriptor.flags & DESC_F_NEXT != 0;
        out.push(descriptor);
        if !has_next {
            return true;
        }
        index = descriptor.next;
        if index >= size {
            out.clear();
            return false;
        }
    }
    out.clear();
    false
}
#[path = "virtio_queue_used.rs"]
mod used;
pub(crate) use used::write_used;
