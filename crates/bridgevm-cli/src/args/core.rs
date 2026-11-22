//! Top-level subcommands.

pub(crate) use super::app_args::*;
pub(crate) use super::cli::*;
pub(crate) use super::hvf_args::*;
use crate::*;

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create and manage VMs in the native macOS app library.
    App(AppArgs),
    #[command(
        subcommand,
        about = "Own-HVF local-only GPU reports",
        long_about = "Own-HVF local-only GPU reports.

These commands read trace and log files and run the synthetic 3D host preflight; they do not create a VM or manage native app VM registrations."
    )]
    Hvf(HvfCommand),
}
