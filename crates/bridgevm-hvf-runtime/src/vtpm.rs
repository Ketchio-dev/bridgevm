//! The supervisor-owned swtpm lifecycle for the typed launch path.
//!
//! The probe's contract is explicit: `BRIDGEVM_SWTPM_DATA_SOCKET` is an
//! opt-in TPM2 TIS backend and *the supervisor owns the swtpm lifecycle*.
//! One swtpm serves every helper generation of a run -- TPM state must
//! survive a guest reset exactly like the disk does -- so it starts before
//! the first generation and stops when the run ends (Drop kills it).

use std::path::{Path, PathBuf};
use std::process::ExitStatus;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(test)]
use std::time::Duration;

#[path = "controlled_swtpm.rs"]
mod controlled;
#[path = "vtpm_wait.rs"]
mod vtpm_wait;
use crate::owned_child::OwnedChild;

use crate::{RuntimeControl, RuntimeError};

/// The durable vTPM state, executable and optional key delivered over the
/// owned child's stdin. Key bytes never enter argv, environment or files.
pub struct VtpmConfig {
    pub state_dir: PathBuf,
    pub swtpm_bin: PathBuf,
    /// The raw AES-256 state key for the product's encrypted vTPM state.
    /// Delivered over swtpm's fd 0 -- never argv, never env, never a file
    /// -- exactly like the wrapper's --swtpm-key-stdin path.
    pub state_key: Option<Vec<u8>>,
}

/// A running swtpm bound to two Unix sockets. Dropping it terminates the
/// process and removes the runtime directory.
pub struct SwtpmProcess {
    child: OwnedChild,
    runtime_dir: PathBuf,
    data_socket: PathBuf,
    control_socket: PathBuf,
}

impl SwtpmProcess {
    pub fn data_socket(&self) -> &Path {
        &self.data_socket
    }
    pub fn control_socket(&self) -> &Path {
        &self.control_socket
    }
    pub fn shutdown(&mut self, control: &RuntimeControl<'_>) -> Result<ExitStatus, RuntimeError> {
        let status = self.child.shutdown(control);
        match std::fs::remove_dir_all(&self.runtime_dir) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(source) => {
                return Err(RuntimeError::Io {
                    context: "remove owned swtpm runtime dir",
                    source,
                })
            }
        }
        Ok(status)
    }
}

impl Drop for SwtpmProcess {
    fn drop(&mut self) {
        let _ = self.shutdown(&RuntimeControl::default());
    }
}

/// A runtime-directory name no other instance in this process can produce.
///
/// This was pid plus `subsec_nanos`, which only distinguishes within one second
/// and collided 177,519 times in 200,000 draws. Two swtpm instances that landed
/// on the same name shared a directory, so dropping either deleted the other's
/// live sockets. A counter makes the name unique by construction instead.
///
/// It also has to stay short. The sockets live inside this directory and a Unix
/// socket path is capped at 104 bytes; on a macOS temp dir a nanosecond
/// timestamp already reached 103, so a two-digit counter would have broken
/// swtpm outright. pid plus counter is both unique and small.
pub(crate) fn unique_runtime_dir_name() -> String {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    format!(
        "bridgevm-vtpm-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    )
}

/// Start swtpm for one run and wait until both sockets exist.
///
/// The socket directory is fresh and private (0700 via tempdir semantics);
/// the state directory is created if missing, like the wrapper's
/// `install -d -m 700`.
pub fn start_swtpm(config: &VtpmConfig) -> Result<SwtpmProcess, RuntimeError> {
    start_swtpm_controlled(config, &RuntimeControl::default())
}

pub use controlled::start_swtpm_controlled;

#[cfg(test)]
#[path = "vtpm_tests.rs"]
mod tests;
