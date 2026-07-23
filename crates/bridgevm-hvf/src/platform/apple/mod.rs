//! Apple Hypervisor.framework backend, split by responsibility.
//!
//! Every `unsafe` site in the crate lives under this module tree, grouped by
//! what it is responsible for rather than in one flat file.

mod diagnostic_vector;
mod ffi;
mod firmware_irq;
mod firmware_run_loop;
mod guest_entry;
mod guest_mem;
mod guest_phys;
mod host;
mod lifecycle;
mod memory_map;
mod mmio_block;
mod mmio_devices;
mod mmio_emulation;
mod pflash;
mod reset_vector;
mod stage1;
mod vcpu_run;

pub(crate) use diagnostic_vector::*;
pub(crate) use ffi::*;
pub(crate) use firmware_irq::*;
pub use firmware_run_loop::*;
pub use guest_entry::*;
pub(crate) use guest_mem::*;
pub(crate) use guest_phys::*;
pub use host::*;
pub(crate) use lifecycle::*;
pub use memory_map::*;
pub(crate) use mmio_block::*;
pub(crate) use mmio_devices::*;
pub(crate) use mmio_emulation::*;
pub use pflash::*;
pub use reset_vector::*;
pub(crate) use stage1::*;
pub(crate) use vcpu_run::*;
