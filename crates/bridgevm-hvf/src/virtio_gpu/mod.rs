//! virtio-gpu device model, decomposed by responsibility: wire protocol,
//! register planes, virtqueue transport, the 2D and blob command families,
//! scanout presentation, interrupts, and diagnostics.
#[cfg(test)]
mod tests;

mod bytes;
mod command;
mod compositor;
mod config_space;
mod device;
pub(crate) mod display;
mod fb_sink;
mod fence;
pub(crate) mod interrupt;
mod pci_device;
mod protocol;
mod registers;
mod resource;
mod scanout;
mod scanout_3d;
mod snapshot;
mod trace;
mod trace_fields;
mod vblank;
mod venus_start_trace;
mod virtqueue;

pub(crate) use bytes::*;
pub(crate) use compositor::*;
pub use device::*;
pub(crate) use fb_sink::*;
pub(crate) use fence::*;
pub use pci_device::*;
pub(crate) use protocol::*;
pub(crate) use registers::*;
pub(crate) use resource::*;
pub use scanout::*;
pub(crate) use scanout_3d::*;
pub(crate) use trace::*;
pub(crate) use trace_fields::*;
pub use vblank::*;
pub(crate) use venus_start_trace::*;
pub(crate) use virtqueue::*;
