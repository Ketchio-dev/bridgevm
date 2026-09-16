//! Forward native commands without a shell or a second VM lifecycle.

use crate::*;
use std::ffi::OsString;
use std::os::unix::process::CommandExt;

pub(crate) fn run(args: AppArgs) -> Result<()> {
    let executable = app_cli_resolver::resolve()?;
    let error = ProcessCommand::new(&executable)
        .args(native_arguments(args))
        .exec();
    Err(error)
        .with_context(|| format!("could not execute native app CLI: {}", executable.display()))
}

fn native_arguments(args: AppArgs) -> Vec<OsString> {
    let (verb, id) = match args.command {
        AppCommand::Query(query) => query.into_parts(),
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

#[cfg(test)]
#[path = "app_cli_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "app_cli_stop_tests.rs"]
mod stop_tests;
