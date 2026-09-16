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

#[path = "app_cli_arguments.rs"]
mod arguments;
use arguments::native_arguments;

#[cfg(test)]
#[path = "app_cli_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "app_cli_stop_tests.rs"]
mod stop_tests;

#[cfg(test)]
#[path = "app_cli_start_tests.rs"]
mod start_tests;
