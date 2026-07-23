use anyhow::Result;

#[cfg(test)]
#[path = "tests_split/mod.rs"]
mod tests;

fn main() -> Result<()> {
    run()
}

mod backend_lifecycle;
mod backend_spawn;
mod cli;
mod daemon_state;
mod guest_tools_protocol;
mod guest_tools_runtime_metadata;
mod guest_tools_session;
mod helper_preflight;
mod ipc_listener;
pub(crate) mod performance_sample;
mod proxy_window_crop_artifacts;
mod proxy_window_crop_config;
mod qmp_supervisor;
mod snapshot_orchestration;
mod spawn_config;
mod supervisor_loop;
mod unix_time;

pub(crate) use backend_spawn::*;
pub(crate) use cli::*;
pub(crate) use daemon_state::*;
pub(crate) use guest_tools_protocol::*;
pub(crate) use guest_tools_runtime_metadata::*;
pub(crate) use guest_tools_session::*;
pub(crate) use helper_preflight::*;
pub(crate) use ipc_listener::*;
pub(crate) use proxy_window_crop_artifacts::*;
pub(crate) use proxy_window_crop_config::*;
pub(crate) use qmp_supervisor::*;
pub(crate) use spawn_config::*;
pub(crate) use supervisor_loop::*;
pub(crate) use unix_time::*;
