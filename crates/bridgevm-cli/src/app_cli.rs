//! Forward native commands without a shell or a second VM lifecycle.

use crate::*;
use std::ffi::OsString;
pub(crate) fn run(args: AppArgs) -> Result<()> {
    doctor::run_or_forward(args)
}

#[path = "app_cli_doctor.rs"]
mod doctor;

#[path = "app_cli_arguments.rs"]
mod arguments;
#[cfg(test)]
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
