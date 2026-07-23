use anyhow::Result;

mod args;

pub(crate) use args::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}
