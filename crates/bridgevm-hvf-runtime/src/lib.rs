//! Product runtime over `bridgevm-hvf`.
//!
//! Exists so the product app and `hvf-runner` stop executing a diagnostic
//! example binary through a shell script (criterion A14). What the product
//! needs from a launch is typed here: a versioned manifest naming the disk,
//! vars and sizing; validation that refuses repository paths and duplicate
//! disk writers before any VM exists; and a reset generation so an event from
//! the previous boot can never be mistaken for one from the current boot.
//!
//! Device models stay in `bridgevm-hvf`. This crate owns the typed process
//! lifecycle, child reaping, reset policy and media leases without a shell.
//! The ordinary runner owns signal policy and supplies cancellation requests.

mod controlled_run;
mod error;
mod manifest;
mod owned_child;
mod owned_control;
mod reset_cycles;
mod reset_generation;
mod reset_receipt;
mod supervisor;
mod vm_builder;
mod vm_event;
mod vm_process;
mod vm_run;
mod vtpm;

pub use error::RuntimeError;
pub use manifest::{LaunchManifest, MANIFEST_VERSION};
pub use owned_control::*;
pub use reset_cycles::{supervise_reset_cycles, HelperExit, ResetCycle};
pub use reset_generation::{GenerationTag, ResetGeneration};
pub use reset_receipt::{flush_and_write_receipt, receipt_proves_flush};
pub use supervisor::{decide_restart, RestartDecision};
pub use vm_builder::{prepare, PreparedVm};
pub use vm_event::{DrainedEvents, StampedEvent, VmEvent, VmEventQueue};
pub use vm_process::{
    helper_env, spawn_helper, DeviceSurfaces, GpuSurface, HelperLaunch, ShareSurface,
};
pub use vm_run::{run_vm, RESET_EXIT_CODE};
pub use vtpm::{start_swtpm, SwtpmProcess, VtpmConfig};
