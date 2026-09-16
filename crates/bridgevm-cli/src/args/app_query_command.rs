//! Read-only native app commands retain their original syntax.

use crate::*;

#[derive(Debug, Subcommand)]
pub(crate) enum AppQueryCommand {
    /// List saved native app VM configurations, including inventory issues.
    List,
    /// Inspect one exact VM ID returned by list, including Korean IDs.
    Inspect { id: String },
    /// Report native launch-input readiness without starting a VM.
    Readiness { id: String },
    /// Query retained runtime observations from the already-running app.
    Status { id: String },
}

impl AppQueryCommand {
    pub(crate) fn into_parts(self) -> (&'static str, Option<String>) {
        match self {
            Self::List => ("list", None),
            Self::Inspect { id } => ("inspect", Some(id)),
            Self::Readiness { id } => ("readiness", Some(id)),
            Self::Status { id } => ("status", Some(id)),
        }
    }
}
