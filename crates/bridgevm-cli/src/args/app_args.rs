//! Native app query and owned-control arguments.

use crate::*;
#[path = "app_command.rs"]
mod app_command;
pub(crate) use app_command::AppCommand;
#[cfg(test)]
pub(crate) use app_command::AppQueryCommand;
#[path = "app_library.rs"]
mod library;
use library::absolute_library;

#[derive(Debug, Parser)]
#[command(
    after_help = "Uses native vm.json registrations, not the legacy --store. Requires a compatible installed BridgeVMControl.app; no window or VM is started. Inventory runtime remains unobserved. Status asks the already-running app for retained observations, not guest health. Stop waits for confirmed cleanup of the exact app-owned runtime; unavailable or incomplete cleanup returns nonzero."
)]
pub(crate) struct AppArgs {
    #[command(subcommand)]
    pub(crate) command: AppCommand,
    /// Native VM library root; must be absolute and contain no '..' component.
    #[arg(long, global = true, value_name = "PATH", value_parser = absolute_library)]
    pub(crate) library: Option<PathBuf>,
    /// Print versioned native JSON for the selected command.
    #[arg(long, global = true)]
    pub(crate) json: bool,
}
