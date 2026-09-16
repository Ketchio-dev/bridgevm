use super::*;

pub(super) fn native_arguments(args: AppArgs) -> Vec<OsString> {
    let (verb, id) = match args.command {
        AppCommand::Query(query) => query.into_parts(),
        AppCommand::Start { id } => ("start", Some(id)),
        AppCommand::Stop { id } => ("stop", Some(id)),
    };
    let mut arguments = vec![OsString::from("--cli"), verb.into()];
    if let Some(id) = id {
        arguments.push(id.into());
    }
    if let Some(library) = args.library {
        arguments.push("--library".into());
        arguments.push(library.into_os_string());
    }
    if args.json {
        arguments.push("--json".into());
    }
    arguments
}
