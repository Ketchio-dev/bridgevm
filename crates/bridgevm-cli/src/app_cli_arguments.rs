use super::*;
#[path = "app_cli_command_parts.rs"]
mod command_parts;

pub(super) fn native_arguments(args: AppArgs) -> Vec<OsString> {
    let (verb, id) = args.command.into_native_parts();
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
