//! Diagnostic probe result types, their evidence renderers, and the safe
//! wrappers that delegate into the cfg-selected `crate::platform` backend.

pub mod host_capabilities;
pub mod lifecycle;
pub mod mmio;
pub mod virtio_block;

pub use host_capabilities::*;
pub use lifecycle::*;
pub use mmio::*;
pub use virtio_block::*;
