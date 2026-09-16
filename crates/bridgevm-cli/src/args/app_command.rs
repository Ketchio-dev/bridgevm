//! Native query and owned-runtime command selection.

use crate::*;
#[path = "app_query_command.rs"]
mod query;
pub(crate) use query::AppQueryCommand;

#[derive(Debug, Subcommand)]
pub(crate) enum AppCommand {
    #[command(flatten)]
    Query(AppQueryCommand),
    /// Stop an app-owned runtime and wait for confirmed owned-process cleanup.
    Stop { id: String },
}
