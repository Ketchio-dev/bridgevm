use super::*;

#[rustfmt::skip]
impl AppCommand {
    pub(super) fn into_native_arguments(self) -> Vec<OsString> {
        let (verb, id) = match self {
            Self::Doctor => unreachable!("app doctor is handled before native command forwarding"), Self::CreateWindows(args) => return args.into_native_arguments(), Self::SnapshotExport(args) => return args.into_native_arguments(), Self::Query(query) => query.into_parts(),
            Self::Start { id } => ("start", Some(id)), Self::Stop { id } => ("stop", Some(id)),
            Self::Install { id } => ("install", Some(id)), Self::InstallStatus { id } => ("install-status", Some(id)),
            Self::InstallCancel { id } => ("install-cancel", Some(id)), Self::SnapshotCreate { id } => ("snapshot-create", Some(id)), Self::SnapshotRestore { id } => ("snapshot-restore", Some(id)),
        };
        super::create_arguments::native_id_arguments(verb, id)
    }
}
