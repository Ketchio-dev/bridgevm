//! Acquire child ownership before key delivery and every startup failure.

use super::{unique_runtime_dir_name, vtpm_wait, SwtpmProcess, VtpmConfig};
use crate::owned_child::OwnedChild;
use crate::{ChildRole, DirectoryDisposition, RuntimeControl, RuntimeError, RuntimeLifecycleEvent};
use std::os::unix::fs::DirBuilderExt;
#[path = "controlled_swtpm_command.rs"]
mod command;
use std::time::{Duration, Instant};
#[path = "controlled_swtpm_io.rs"]
pub(super) mod io;

pub fn start_swtpm_controlled(
    config: &VtpmConfig,
    control: &RuntimeControl<'_>,
) -> Result<SwtpmProcess, RuntimeError> {
    if config.state_key.as_ref().is_some_and(|key| key.len() != 32) {
        return Err(failure(
            "validate swtpm state key",
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "state key must be exactly 32 bytes",
            ),
        ));
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    io::check(deadline, control).map_err(|source| failure("swtpm startup cancelled", source))?;
    std::fs::create_dir_all(&config.state_dir)
        .map_err(|source| failure("create vTPM state dir", source))?;
    let runtime_dir = std::env::temp_dir().join(unique_runtime_dir_name());
    std::fs::DirBuilder::new()
        .mode(0o700)
        .create(&runtime_dir)
        .map_err(|source| failure("create swtpm runtime dir", source))?;
    control.observe(RuntimeLifecycleEvent::SwtpmDirectory(
        DirectoryDisposition::Created,
    ));
    let data_socket = runtime_dir.join("data.sock");
    let control_socket = runtime_dir.join("control.sock");
    let mut command = command::build(config, &data_socket, &control_socket);
    let spawned = io::check(deadline, control).and_then(|()| command.spawn());
    let child = match spawned {
        Ok(child) => OwnedChild::adopt(child, ChildRole::Swtpm, None, control),
        Err(source) => {
            super::remove_runtime_dir(&runtime_dir, control).ok();
            return Err(failure("spawn swtpm", source));
        }
    };
    let mut process = SwtpmProcess {
        child,
        runtime_dir,
        data_socket,
        control_socket,
    };
    let ready = (|| {
        if let Some(key) = &config.state_key {
            let mut stdin = process.child.take_stdin().ok_or_else(|| {
                failure(
                    "open swtpm key pipe",
                    std::io::Error::other("stdin not piped"),
                )
            })?;
            io::write_key(&mut stdin, key, deadline, control)
                .map_err(|source| failure("deliver swtpm state key", source))?;
        }
        vtpm_wait::wait_for_sockets(&mut process, deadline, control)
    })();
    if let Err(error) = ready {
        if let Err(cleanup) = process.shutdown(control) {
            return Err(failure(
                "swtpm startup and cleanup failed",
                std::io::Error::other(format!("{error}; cleanup: {cleanup}")),
            ));
        }
        return Err(error);
    }
    Ok(process)
}

fn failure(context: &'static str, source: std::io::Error) -> RuntimeError {
    RuntimeError::Io { context, source }
}

pub(super) fn require_live(
    process: &mut SwtpmProcess,
    context: &'static str,
    control: &RuntimeControl<'_>,
) -> Result<(), RuntimeError> {
    match process.child.try_wait_observed(control) {
        Ok(None) => Ok(()),
        Ok(Some(status)) => Err(failure(
            context,
            std::io::Error::other(format!("exit status {status}")),
        )),
        Err(source) => Err(failure("observe swtpm startup", source)),
    }
}

#[cfg(test)]
#[path = "controlled_swtpm_tests.rs"]
mod tests;
