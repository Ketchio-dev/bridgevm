//! Native app query and owned-control arguments.

use crate::*;
#[path = "app_command.rs"]
mod app_command;
#[cfg(test)]
pub(crate) use app_command::AppQueryCommand;
pub(crate) use app_command::{AppCommand, AppCreateWindowsArgs};
#[path = "app_library.rs"]
mod library;
use library::absolute_library;

#[derive(Debug, Parser)]
#[command(
    after_help = "Uses native vm.json registrations, not the legacy --store. Requires a compatible installed BridgeVMControl.app; no app window is opened. Create accepts one non-secret absolute ISO path and copies it into the owned VM bundle; snapshot-export accepts one non-secret absolute output directory. Passwords and recovery keys are never accepted. Install uses only the pending request already saved with the VM. Start confirms initial helper startup, not guest boot. Status asks the app for retained observations, not guest health. Stop waits for confirmed owned-process cleanup."
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
