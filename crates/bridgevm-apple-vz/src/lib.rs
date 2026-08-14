//! Planning and launch support for the Apple Virtualization framework engine.
//!
//! Turns a VM manifest into a launch specification for the out-of-process
//! `AppleVzRunner` helper and handles the handoff. The framework itself is driven
//! from Swift, because Virtualization.framework has no Rust binding.

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

mod boot_spec;
mod bundle_paths;
mod command_launcher;
mod errors;
mod handoff;
mod launch_spec;
mod launch_spec_artifact;
mod launcher;
mod network_plan;
mod plan;
mod preflight;
mod readiness;
mod shares;

pub(crate) use boot_spec::*;
pub(crate) use bundle_paths::*;
pub use command_launcher::*;
pub use errors::*;
pub use handoff::*;
pub use launch_spec::*;
pub use launch_spec_artifact::*;
pub use launcher::*;
pub(crate) use network_plan::*;
pub use plan::*;
pub(crate) use preflight::*;
pub(crate) use readiness::*;
pub use shares::*;
