use anyhow::Result;

mod clipboardwriter;
mod effect_command_path;
mod safe_file_drop_destination;
mod xdotool_button;

pub(crate) use clipboardwriter::*;
pub(crate) use effect_command_path::*;
pub(crate) use safe_file_drop_destination::*;
pub(crate) use xdotool_button::*;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}

mod xdotool_button_impl_2;
