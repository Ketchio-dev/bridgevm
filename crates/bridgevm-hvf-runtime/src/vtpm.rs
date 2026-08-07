//! The supervisor-owned swtpm lifecycle for the typed launch path.
//!
//! The probe's contract is explicit: `BRIDGEVM_SWTPM_DATA_SOCKET` is an
//! opt-in TPM2 TIS backend and *the supervisor owns the swtpm lifecycle*.
//! One swtpm serves every helper generation of a run -- TPM state must
//! survive a guest reset exactly like the disk does -- so it starts before
//! the first generation and stops when the run ends (Drop kills it).

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use crate::RuntimeError;

/// What a launch says about the vTPM: where durable state lives and which
/// swtpm binary to run. No key support here yet -- the app's encrypted
/// state path still goes through the wrapper (key-over-fd is its own slice).
pub struct VtpmConfig {
    pub state_dir: PathBuf,
    pub swtpm_bin: PathBuf,
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

/// Start swtpm for one run and wait until both sockets exist.
///
/// The socket directory is fresh and private (0700 via tempdir semantics);
/// the state directory is created if missing, like the wrapper's
/// `install -d -m 700`.
pub fn start_swtpm(config: &VtpmConfig) -> Result<SwtpmProcess, RuntimeError> {
    let io =
        |context: &'static str| move |source: std::io::Error| RuntimeError::Io { context, source };
    std::fs::create_dir_all(&config.state_dir).map_err(io("create vTPM state dir"))?;
    let runtime_dir = std::env::temp_dir().join(format!(
        "bridgevm-vtpm-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.subsec_nanos())
            .unwrap_or(0)
    ));
    std::fs::create_dir_all(&runtime_dir).map_err(io("create swtpm runtime dir"))?;
    let data_socket = runtime_dir.join("data.sock");
    let control_socket = runtime_dir.join("control.sock");
    let child = Command::new(&config.swtpm_bin)
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
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(io("spawn swtpm"))?;
    let mut process = SwtpmProcess {
        child,
        runtime_dir,
        data_socket,
        control_socket,
    };
    // The wrapper polls 100 * 50ms; same budget here.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if process.data_socket.exists() && process.control_socket.exists() {
            return Ok(process);
        }
        if let Ok(Some(status)) = process.child.try_wait() {
            return Err(RuntimeError::Io {
                context: "swtpm exited before creating its sockets",
                source: std::io::Error::other(format!("exit status {status}")),
            });
        }
        if Instant::now() >= deadline {
            return Err(RuntimeError::Io {
                context: "swtpm socket wait",
                source: std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    "sockets not created within 5s",
                ),
            });
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

#[cfg(test)]
#[path = "vtpm_tests.rs"]
mod tests;
