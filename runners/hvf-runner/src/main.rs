use anyhow::Result;
mod args;
mod launch_spec;
#[cfg(debug_assertions)]
mod resolve_launch_path;
#[cfg(debug_assertions)]
mod supervise_cli;
pub(crate) use args::*;
#[cfg(debug_assertions)]
pub(crate) use resolve_launch_path::*;
#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}
