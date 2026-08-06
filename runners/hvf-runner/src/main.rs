use anyhow::Result;

mod args;
mod launch_spec;
mod resolve_launch_path;

pub(crate) use args::*;
pub(crate) use resolve_launch_path::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}
