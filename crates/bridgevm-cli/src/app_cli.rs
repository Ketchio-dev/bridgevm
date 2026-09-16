//! Forward read-only native queries without a shell or a second process lifecycle.

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
    let mut arguments = vec![OsString::from("--cli")];
    match args.command {
        AppCommand::List => arguments.push("list".into()),
        AppCommand::Inspect { id } => {
            arguments.push("inspect".into());
            arguments.push(id.into());
        }
        AppCommand::Readiness { id } => {
            arguments.push("readiness".into());
            arguments.push(id.into());
        }
        AppCommand::Status { id } => {
            arguments.push("status".into());
            arguments.push(id.into());
        }
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
