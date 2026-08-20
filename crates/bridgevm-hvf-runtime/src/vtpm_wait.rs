//! Waiting for swtpm to be genuinely ready, not merely to have bound sockets.

use super::*;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

/// swtpm enters its control loop only after it has loaded/decrypted TPM state.
fn control_ready(path: &Path) -> bool {
    let Ok(mut stream) = UnixStream::connect(path) else {
        return false;
    };
    let mut response = [0u8; 8];
    stream
        .set_read_timeout(Some(Duration::from_millis(100)))
        .is_ok()
        && stream.write_all(&1u32.to_be_bytes()).is_ok()
        && stream.read_exact(&mut response).is_ok()
        && response[..4] == [0; 4]
        && response[4..] != [0; 4]
}

pub(crate) fn wait_for_sockets(mut process: SwtpmProcess) -> Result<SwtpmProcess, RuntimeError> {
    // The wrapper polls 100 * 50ms; same budget here.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if process.data_socket.exists() && control_ready(&process.control_socket) {
            return match process.child.try_wait() {
                Ok(Some(status)) => Err(RuntimeError::Io {
                    context: "swtpm exited after creating its sockets",
                    source: std::io::Error::other(format!("exit status {status}")),
                }),
                _ => Ok(process),
            };
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
