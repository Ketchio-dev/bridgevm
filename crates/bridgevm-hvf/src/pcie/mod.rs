//! pcie, decomposed by responsibility.
#[cfg(test)]
mod tests;
mod virtio_caps;

mod bar;
mod bar_routing;
mod capabilities;
mod cfg_addr;
mod cfg_regs;
mod ecam;
mod endpoint_ids;
mod endpoint_queries;
mod function;
mod function_builders;
mod msix_capability;
pub(crate) mod snapshot;
mod trace;

pub(crate) use bar::*;
pub use bar_routing::*;
pub use capabilities::*;
pub use cfg_addr::*;
pub use cfg_regs::*;
pub use ecam::*;
pub use endpoint_ids::*;
pub use endpoint_queries::*;
pub(crate) use function::*;
pub use msix_capability::*;
pub(crate) use trace::*;
