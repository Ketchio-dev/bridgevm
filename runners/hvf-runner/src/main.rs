use anyhow::Result;

mod args;
mod resolve_launch_path;

pub(crate) use args::*;
pub(crate) use resolve_launch_path::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}
