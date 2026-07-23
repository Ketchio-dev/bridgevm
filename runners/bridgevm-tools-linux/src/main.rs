use anyhow::Result;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}

mod benchmark;
mod bounded_process;
mod cli;
mod clipboard_effects;
mod clock_effects;
mod desktop_commands;
mod desktop_controller;
mod desktop_input;
mod desktop_inventory;
mod display_resize_effects;
mod effect_backend_detection;
mod filesystem_freeze_effects;
mod guest_tools_state;
mod handshake;
mod session_loop;
pub(crate) mod share_and_file_drop_commands;
mod system_commands;
mod telemetry;

pub(crate) use benchmark::*;
pub(crate) use bounded_process::*;
pub(crate) use cli::*;
pub(crate) use clipboard_effects::*;
pub(crate) use clock_effects::*;
pub(crate) use desktop_controller::*;
pub(crate) use desktop_input::*;
pub(crate) use desktop_inventory::*;
pub(crate) use display_resize_effects::*;
pub(crate) use effect_backend_detection::*;
pub(crate) use filesystem_freeze_effects::*;
pub(crate) use guest_tools_state::*;
pub(crate) use handshake::*;
pub(crate) use session_loop::*;
pub(crate) use telemetry::*;
