//! Product runtime over `bridgevm-hvf`.
//!
//! Exists so the product app and `hvf-runner` stop executing a diagnostic
//! example binary through a shell script (criterion A14). What the product
//! needs from a launch is typed here: a versioned manifest naming the disk,
//! vars and sizing; validation that refuses repository paths and duplicate
//! disk writers before any VM exists; and a reset generation so an event from
//! the previous boot can never be mistaken for one from the current boot.
//!
//! Device models stay in `bridgevm-hvf`. This crate owns lifecycle and
//! policy, and deliberately has no shell or process-launch dependency: a
//! supervisor that recreates the helper process lives with the process it
//! recreates, not here.

mod error;
mod manifest;
mod reset_cycles;
mod reset_generation;
mod reset_receipt;
mod supervisor;
mod vm_builder;
mod vm_event;
mod vm_process;

pub use error::RuntimeError;
pub use manifest::{LaunchManifest, MANIFEST_VERSION};
pub use reset_cycles::{supervise_reset_cycles, HelperExit, ResetCycle};
pub use reset_generation::{GenerationTag, ResetGeneration};
pub use reset_receipt::{flush_and_write_receipt, receipt_proves_flush};
pub use supervisor::{decide_restart, RestartDecision};
pub use vm_builder::{prepare, PreparedVm};
pub use vm_event::{DrainedEvents, StampedEvent, VmEvent, VmEventQueue};
pub use vm_process::{helper_env, spawn_helper, HelperLaunch};
