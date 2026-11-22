//! Top-level command dispatch.

use crate::*;

pub(crate) fn run() -> Result<()> {
    match Cli::parse().command {
        Command::App(args) => app_cli::run(args),
        Command::Hvf(args) => hvf(args),
    }
}
