//! Allocation-free reads from fragmented virtio-gpu resource backing.

use super::backing_copy_cursor::{read_from_backing_into_from, BackingReadCursor};
use super::BackingEntry;
use crate::fwcfg::GuestMemoryMut;

pub(crate) fn read_from_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BackingEntry],
    offset: u64,
    dst: &mut [u8],
) -> bool {
    read_from_backing_into_from(mem, backing, offset, dst, &mut BackingReadCursor::default())
}

#[cfg(test)]
#[path = "backing_copy_tests.rs"]
mod tests;
