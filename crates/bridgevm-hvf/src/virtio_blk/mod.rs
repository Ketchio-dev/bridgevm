//! virtio_blk, split for the 1000-line rule.
mod trace;
pub use trace::{VirtioBlockRequestTrace, RECENT_REQUEST_TRACE_LIMIT};
#[cfg(test)]
mod tests;

mod installer_iso_slot;
mod pci_to_mmio_offset;

pub use installer_iso_slot::*;
pub(crate) use pci_to_mmio_offset::*;

mod installer_iso_slot_impl_2;
