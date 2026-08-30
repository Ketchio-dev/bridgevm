//! virtio-gpu device model, decomposed by responsibility: wire protocol,
//! register planes, queues, command families, presentation and diagnostics.
#[cfg(test)]
mod tests;

mod async_present;
mod bytes;
mod command;
mod compositor;
mod config_space;
mod descriptor_chain_trace;
mod device;
pub(crate) mod display;
mod fb_sink;
mod fence;
pub(crate) mod interrupt;
mod pci_device;
mod protocol;
mod queue_pending;
mod registers;
mod resource;
mod scanout;
mod scanout_3d;
mod scanout_async;
mod scanout_async_apply;
mod scanout_blit;
mod scanout_readback;
mod snapshot;
mod trace;
mod trace_clock;
mod trace_fields;
mod vblank;
mod vblank_wake_state;
mod venus_start_trace;
mod virtqueue;

pub(crate) use async_present::*;
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
use vblank::PendingVblankResponse;
pub use vblank_wake_state::*;
pub(crate) use venus_start_trace::*;
pub(crate) use virtqueue::*;
