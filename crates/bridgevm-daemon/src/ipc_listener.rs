//! Unix-socket bind and accept, per-client worker threads, newline-framed request/response codec.

use anyhow::Context;
use anyhow::Result;
use bridgevm_api::BridgeVmRequest;
use std::fs;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
// only the #[cfg(test)] connection helper below names this type
#[cfg(test)]
use crate::daemon_state::DaemonState;
use bridgevm_api::BridgeVmResponse;
use std::io::BufRead;
use std::io::Write;
use std::os::unix::fs::FileTypeExt;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::sync::atomic::AtomicUsize;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

pub(crate) const MAX_DAEMON_FRAME_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) const DAEMON_CLIENT_IO_TIMEOUT: Duration = Duration::from_secs(2);

pub(crate) const DAEMON_RESPONSE_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

pub(crate) const MAX_CONCURRENT_DAEMON_CLIENTS: usize = 32;

pub(crate) struct PendingDaemonRequest {
    pub(crate) request: BridgeVmRequest,
    pub(crate) response_sender: mpsc::Sender<BridgeVmResponse>,
}

pub(crate) fn spawn_connection_worker(
    stream: UnixStream,
    request_sender: mpsc::Sender<PendingDaemonRequest>,
    active_clients: Arc<AtomicUsize>,
) {
    if active_clients.fetch_add(1, Ordering::AcqRel) >= MAX_CONCURRENT_DAEMON_CLIENTS {
        active_clients.fetch_sub(1, Ordering::AcqRel);
        return;
    }
    let worker_clients = Arc::clone(&active_clients);
    let spawn_result = thread::Builder::new()
        .name("bridgevmd-client".to_string())
        .spawn(move || {
            if let Err(error) = run_connection_worker(stream, request_sender) {
                eprintln!("bridgevmd request failed: {error:#}");
            }
            worker_clients.fetch_sub(1, Ordering::AcqRel);
        });
    if let Err(error) = spawn_result {
        active_clients.fetch_sub(1, Ordering::AcqRel);
        eprintln!("bridgevmd failed to spawn client worker: {error}");
    }
}

pub(crate) fn run_connection_worker(
    mut stream: UnixStream,
    request_sender: mpsc::Sender<PendingDaemonRequest>,
) -> Result<()> {
    // The listener is nonblocking so the supervisor can keep reconciling
    // children and observe shutdown requests.  On macOS an accepted stream
    // can inherit O_NONBLOCK from that listener; restore blocking I/O before
    // applying finite timeouts so a client that has connected but has not yet
    // written its frame is not rejected with EAGAIN.
    stream
        .set_nonblocking(false)
        .context("failed to configure daemon client blocking mode")?;
    stream
        .set_read_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))
        .context("failed to configure daemon client read timeout")?;
    stream
        .set_write_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))
        .context("failed to configure daemon client write timeout")?;
    let request = read_daemon_request(&stream)?;
    let (response_sender, response_receiver) = mpsc::channel();
    request_sender
        .send(PendingDaemonRequest {
            request,
            response_sender,
        })
        .context("daemon supervisor stopped before handling request")?;
    let response = response_receiver
        .recv_timeout(DAEMON_RESPONSE_WAIT_TIMEOUT)
        .context("daemon supervisor did not return a response before timeout")?;
    write_daemon_response(&mut stream, &response)
}

pub(crate) fn bind_daemon_listener(socket_path: &Path) -> Result<UnixListener> {
    if let Some(parent) = socket_path.parent() {
        fs::create_dir_all(parent).context("failed to create daemon run directory")?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .context("failed to protect daemon run directory")?;
    }
    if socket_path.exists() {
        let metadata = match fs::symlink_metadata(socket_path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == ErrorKind::NotFound => {
                return bind_new_daemon_listener(socket_path);
            }
            Err(error) => {
                return Err(error).context("failed to inspect existing daemon socket path");
            }
        };
        if !metadata.file_type().is_socket() {
            anyhow::bail!(
                "daemon socket path exists and is not a socket: {}",
                socket_path.display()
            );
        }
        match UnixStream::connect(socket_path) {
            Ok(_) => {
                anyhow::bail!("daemon socket already in use: {}", socket_path.display());
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(error) if error.kind() == ErrorKind::ConnectionRefused => {
                fs::remove_file(socket_path).context("failed to remove stale daemon socket")?;
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to connect to existing daemon socket: {}",
                        socket_path.display()
                    )
                });
            }
        }
    }

    bind_new_daemon_listener(socket_path)
}

pub(crate) fn bind_new_daemon_listener(socket_path: &Path) -> Result<UnixListener> {
    let listener = UnixListener::bind(socket_path).context("failed to bind daemon socket")?;
    if let Err(error) = fs::set_permissions(socket_path, fs::Permissions::from_mode(0o600)) {
        drop(listener);
        let _ = fs::remove_file(socket_path);
        return Err(error).context("failed to protect daemon socket");
    }
    Ok(listener)
}

#[cfg(test)]
pub(crate) fn handle_connection(state: &mut DaemonState, mut stream: UnixStream) -> Result<()> {
    stream.set_read_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))?;
    stream.set_write_timeout(Some(DAEMON_CLIENT_IO_TIMEOUT))?;
    let request = read_daemon_request(&stream)?;
    let response = state.handle_request(request);
    write_daemon_response(&mut stream, &response)
}

pub(crate) fn read_daemon_request(stream: &UnixStream) -> Result<BridgeVmRequest> {
    let mut frame = Vec::new();
    BufReader::new(stream.try_clone()?)
        .take(MAX_DAEMON_FRAME_BYTES + 1)
        .read_until(b'\n', &mut frame)
        .context("failed to read daemon request")?;
    if frame.is_empty() {
        anyhow::bail!("daemon client sent an empty request");
    }
    if frame.len() as u64 > MAX_DAEMON_FRAME_BYTES {
        anyhow::bail!("daemon request exceeded {MAX_DAEMON_FRAME_BYTES} bytes");
    }
    if frame.last() != Some(&b'\n') {
        anyhow::bail!("daemon client sent an incomplete request frame");
    }
    serde_json::from_slice::<BridgeVmRequest>(&frame).context("invalid request JSON")
}

pub(crate) fn write_daemon_response(
    stream: &mut UnixStream,
    response: &BridgeVmResponse,
) -> Result<()> {
    let mut frame = serde_json::to_vec(response).context("failed to encode daemon response")?;
    frame.push(b'\n');
    if frame.len() as u64 > MAX_DAEMON_FRAME_BYTES {
        anyhow::bail!("daemon response exceeded {MAX_DAEMON_FRAME_BYTES} bytes");
    }
    stream
        .write_all(&frame)
        .context("failed to write daemon response")
}
