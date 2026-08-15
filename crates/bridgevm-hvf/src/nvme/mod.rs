//! NVMe 1.4 controller model, decomposed by responsibility: wire protocol,
//! BAR registers, queues, admin command families, the I/O data path,
//! namespaces and their backing store, and diagnostics.
mod admin;
mod controller;
mod disk;
mod features;
mod features_get;
mod identify;
mod interrupts;
mod io;
mod log_page;
mod namespace;
mod protocol;
mod prp;
mod queue;
mod registers;
mod snapshot;
mod trace;

#[cfg(test)]
mod tests;

pub use admin::*;
pub use controller::*;
pub(crate) use disk::*;
pub(crate) use identify::*;
pub use interrupts::*;
pub use namespace::*;
pub use protocol::*;
pub(crate) use prp::*;
pub use queue::*;
pub use registers::*;
pub use trace::*;
