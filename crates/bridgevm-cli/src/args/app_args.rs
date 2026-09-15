//! Read-only native app inventory arguments.

use crate::*;

#[derive(Debug, Parser)]
#[command(
    after_help = "Reads native vm.json registrations, not the legacy --store. Requires a compatible installed BridgeVMControl.app; no window or VM is started. Runtime state remains unobserved."
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

#[derive(Debug, Subcommand)]
pub(crate) enum AppCommand {
    /// List saved native app VM configurations, including inventory issues.
    List,
    /// Inspect one exact VM ID returned by list, including Korean IDs.
    Inspect { id: String },
    /// Report native launch-input readiness without starting a VM.
    Readiness { id: String },
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
