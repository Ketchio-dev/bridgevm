//! Read-only native command selection.

use crate::*;

#[derive(Debug, Subcommand)]
pub(crate) enum AppCommand {
    /// List saved native app VM configurations, including inventory issues.
    List,
    /// Inspect one exact VM ID returned by list, including Korean IDs.
    Inspect { id: String },
    /// Report native launch-input readiness without starting a VM.
    Readiness { id: String },
    /// Query retained runtime observations from the already-running app.
    Status { id: String },
}
