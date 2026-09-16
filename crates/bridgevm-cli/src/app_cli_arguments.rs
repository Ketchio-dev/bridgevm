use super::*;
#[path = "app_cli_command_parts.rs"]
mod command_parts;
#[path = "app_cli_create_arguments.rs"]
mod create_arguments;

pub(super) fn native_arguments(args: AppArgs) -> Vec<OsString> {
    let mut arguments = args.command.into_native_arguments();
    if let Some(library) = args.library {
        arguments.push("--library".into());
        arguments.push(library.into_os_string());
    }
    if args.json {
        arguments.push("--json".into());
    }
    arguments
}
