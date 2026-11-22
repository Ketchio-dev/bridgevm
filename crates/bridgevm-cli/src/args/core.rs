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
        about = "Own-HVF local-only queries and opt-in probes",
        long_about = "Own-HVF local-only queries and opt-in probes.

Host-capabilities and plan commands inspect metadata. Probes may access files or create a VM when their documented opt-ins are supplied. These commands do not manage native app VM registrations."
    )]
    Hvf(HvfCommand),
}
