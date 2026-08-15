//! The supervisor-owned swtpm lifecycle for the typed launch path.
//!
//! The probe's contract is explicit: `BRIDGEVM_SWTPM_DATA_SOCKET` is an
//! opt-in TPM2 TIS backend and *the supervisor owns the swtpm lifecycle*.
//! One swtpm serves every helper generation of a run -- TPM state must
//! survive a guest reset exactly like the disk does -- so it starts before
//! the first generation and stops when the run ends (Drop kills it).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

#[path = "vtpm_wait.rs"]
mod vtpm_wait;
use std::time::{Duration, Instant};
use vtpm_wait::wait_for_sockets;

use crate::RuntimeError;

/// What a launch says about the vTPM: where durable state lives and which
/// swtpm binary to run. No key support here yet -- the app's encrypted
/// state path still goes through the wrapper (key-over-fd is its own slice).
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
    child: Child,
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
}

impl Drop for SwtpmProcess {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        let _ = std::fs::remove_dir_all(&self.runtime_dir);
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
    let io =
        |context: &'static str| move |source: std::io::Error| RuntimeError::Io { context, source };
    std::fs::create_dir_all(&config.state_dir).map_err(io("create vTPM state dir"))?;
    let runtime_dir = std::env::temp_dir().join(unique_runtime_dir_name());
    std::fs::create_dir_all(&runtime_dir).map_err(io("create swtpm runtime dir"))?;
    let data_socket = runtime_dir.join("data.sock");
    let control_socket = runtime_dir.join("control.sock");
    let mut command = Command::new(&config.swtpm_bin);
    command
        .arg("socket")
        .arg("--tpm2")
        .arg("--tpmstate")
        .arg(format!("dir={}", config.state_dir.display()))
        .arg("--server")
        .arg(format!(
            "type=unixio,path={},mode=0600",
            data_socket.display()
        ))
        .arg("--ctrl")
        .arg(format!(
            "type=unixio,path={},mode=0600",
            control_socket.display()
        ))
        .arg("--flags")
        .arg("not-need-init,startup-clear")
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if config.state_key.is_some() {
        command
            .arg("--key")
            .arg("fd=0,format=binary,mode=aes-256-cbc")
            .stdin(Stdio::piped());
    } else {
        command.stdin(Stdio::null());
    }
    let mut child = command.spawn().map_err(io("spawn swtpm"))?;
    if let Some(key) = &config.state_key {
        // Write the key and close the pipe: swtpm reads fd 0 to EOF. The
        // handle drops at the end of this block, so the key exists only in
        // swtpm's memory afterwards.
        use std::io::Write;
        let mut stdin = child.stdin.take().ok_or_else(|| RuntimeError::Io {
            context: "open swtpm key pipe",
            source: std::io::Error::other("stdin not piped"),
        })?;
        stdin
            .write_all(key)
            .map_err(io("deliver swtpm state key"))?;
    }
    let process = SwtpmProcess {
        child,
        runtime_dir,
        data_socket,
        control_socket,
    };
    wait_for_sockets(process)
}

#[cfg(test)]
#[path = "vtpm_tests.rs"]
mod tests;
