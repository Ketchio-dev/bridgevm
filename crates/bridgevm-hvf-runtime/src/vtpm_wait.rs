//! Waiting for swtpm to be genuinely ready, not merely to have bound sockets.

use super::*;

/// How long swtpm must stay alive after its sockets appear before the handle is
/// trusted. It binds the sockets before decrypting the state directory, so a
/// wrong key produces a short-lived process that briefly looks healthy.
const SOCKET_SETTLE: Duration = Duration::from_millis(150);

pub(crate) fn wait_for_sockets(mut process: SwtpmProcess) -> Result<SwtpmProcess, RuntimeError> {
    // The wrapper polls 100 * 50ms; same budget here.
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut sockets_seen_at: Option<Instant> = None;
    loop {
        if process.data_socket.exists() && process.control_socket.exists() {
            // swtpm binds its sockets before it decrypts the state directory,
            // so returning here the instant they appear can hand back a handle
            // to a process that is about to exit because the key was wrong.
            // Give it a short grace period and re-check that it is still alive:
            // without this, a wrong key was accepted in 5 of 12 runs under
            // parallel load.
            let seen = *sockets_seen_at.get_or_insert_with(Instant::now);
            if seen.elapsed() >= SOCKET_SETTLE {
                return match process.child.try_wait() {
                    Ok(Some(status)) => Err(RuntimeError::Io {
                        context: "swtpm exited after creating its sockets",
                        source: std::io::Error::other(format!("exit status {status}")),
                    }),
                    _ => Ok(process),
                };
            }
            std::thread::sleep(Duration::from_millis(10));
            continue;
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
