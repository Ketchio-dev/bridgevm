use super::*;

impl AppCommand {
    pub(super) fn into_native_parts(self) -> (&'static str, Option<String>) {
        match self {
            Self::Query(query) => query.into_parts(),
            Self::Start { id } => ("start", Some(id)),
            Self::Stop { id } => ("stop", Some(id)),
            Self::Install { id } => ("install", Some(id)),
            Self::InstallStatus { id } => ("install-status", Some(id)),
            Self::InstallCancel { id } => ("install-cancel", Some(id)),
        }
    }
}
