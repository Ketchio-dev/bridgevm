//! Read-only native app query arguments.

use crate::*;
#[path = "app_command.rs"]
mod app_command;
pub(crate) use app_command::AppCommand;

#[derive(Debug, Parser)]
#[command(
    after_help = "Reads native vm.json registrations, not the legacy --store. Requires a compatible installed BridgeVMControl.app; no window or VM is started. Inventory runtime remains unobserved. Status asks the already-running app for retained observations, not guest health."
)]
pub(crate) struct AppArgs {
    #[command(subcommand)]
    pub(crate) command: AppCommand,
    /// Native VM library root; must be absolute and contain no '..' component.
    #[arg(long, global = true, value_name = "PATH", value_parser = absolute_library)]
    pub(crate) library: Option<PathBuf>,
    /// Print versioned native JSON for the selected query.
    #[arg(long, global = true)]
    pub(crate) json: bool,
}

fn absolute_library(value: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("--library requires an absolute path without '..'".into());
    }
    Ok(path)
}
