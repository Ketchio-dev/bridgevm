//! virtio-gpu 3D/virgl backend, decomposed by responsibility.
#[cfg(test)]
mod tests;

mod backend;
mod blob_host_mapping;
mod blob_resource;
mod capset;
mod command_dispatch;
mod context;
mod device;
mod fences;
mod local_resource_copy;
mod protocol;
pub(crate) mod resource_3d;
mod scanout_present;
mod submit_3d;
pub(crate) mod trace;

pub use backend::*;
pub use blob_host_mapping::*;
pub use blob_resource::*;
pub use device::*;
pub(crate) use local_resource_copy::*;
pub use protocol::*;
pub(crate) use trace::*;

// The mocks are test-only; gating the module keeps the per-item
// #[cfg(test)] attributes honest and keeps the re-export out of the lib build.
#[cfg(test)]
mod test_mocks;
#[cfg(test)]
pub use test_mocks::*;
