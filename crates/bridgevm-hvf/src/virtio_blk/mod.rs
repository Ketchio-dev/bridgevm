//! virtio_blk, decomposed by responsibility.
mod trace;
pub use trace::{VirtioBlockRequestTrace, RECENT_REQUEST_TRACE_LIMIT};
#[cfg(test)]
mod tests;

pub(crate) mod block_requests;
mod device_state;
pub(crate) mod legacy_transport;
mod media_backend;
mod mmio_regs;
mod pci_transport;
mod protocol;
pub(crate) mod snapshot;
mod virtqueue;

pub use device_state::*;
pub(crate) use media_backend::*;
pub use mmio_regs::*;
pub use pci_transport::*;
pub use protocol::*;
pub(crate) use virtqueue::*;
