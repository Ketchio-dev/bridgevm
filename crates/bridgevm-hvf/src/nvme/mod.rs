//! NVMe 1.4 controller model, decomposed by responsibility: wire protocol,
//! BAR registers, queues, admin command families, the I/O data path,
//! namespaces and their backing store, and diagnostics.
mod admin;
mod admin_data;
mod completion_capacity;
mod configuration;
mod controller;
mod dataset_management;
mod disk;
mod doorbell;
mod features;
mod features_apst;
mod features_get;
mod flush_mode;
mod flush_worker;
mod identify;
mod identify_command;
mod interrupts;
mod io;
mod io_worker;
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
mod unlocked_io;

#[cfg(test)]
mod tests;

pub use admin::*;
pub use controller::*;
pub use dataset_management::dsm_record;
pub(crate) use disk::*;
pub use flush_mode::host_sync_label;
pub(crate) use flush_worker::FlushWorker;
pub use flush_worker::FLUSH_DONE_HOOK;
pub(crate) use identify::*;
pub use interrupts::*;
pub use io_worker::{NvmeIoWorker, NVME_IO_WAIT};
pub use namespace::*;
pub use protocol::*;
pub(crate) use prp::*;
pub use queue::*;
pub use registers::*;
pub use trace::*;
pub use unlocked_io::NvmeIo;
