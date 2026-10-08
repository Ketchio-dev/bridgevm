//! NVMe 1.4 controller model, decomposed by responsibility: wire protocol,
//! BAR registers, queues, admin command families, the I/O data path,
//! namespaces and their backing store, and diagnostics.
mod admin;
mod admin_data;
mod completion_capacity;
mod configuration;
mod controller;
mod disk;
mod doorbell;
mod features;
mod features_get;
mod identify;
mod identify_command;
mod interrupts;
mod io;
mod io_write;
mod log_page;
mod namespace;
mod protocol;
mod prp;
mod queue;
mod queue_create;
mod queue_geometry;
mod queue_lifecycle;
mod registers;
mod snapshot;
mod stream;
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
