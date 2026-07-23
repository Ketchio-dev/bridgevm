//! The guest-tools socket handshake, frame drain, and host-to-guest command send and await.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agentd::accept_guest_hello;
use bridgevm_agentd::decode_envelope_line;
use bridgevm_agentd::read_envelope_line;
use bridgevm_agentd::write_envelope_line;
use bridgevm_agentd::AgentSessionIoError;
use bridgevm_api::guest_tools_agent_policy;
use bridgevm_api::BridgeVmResponse;
use bridgevm_api::GuestToolsCommandRecord;
use bridgevm_storage::VmStore;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::thread;
use std::time::Duration;
use std::time::Instant;

pub(crate) fn command_result_timeout(freeze_timeout_millis: Option<u64>) -> Duration {
    freeze_timeout_millis
        .map(|millis| Duration::from_millis(millis).saturating_add(Duration::from_secs(1)))
        .unwrap_or(GUEST_TOOLS_COMMAND_RESULT_TIMEOUT)
}

pub(crate) fn reconcile_guest_tools_session(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
) -> Result<()> {
    if backend.guest_tools.is_some() {
        return Ok(());
    }

    let metadata = store
        .guest_tools_runner_metadata(name)
        .context("failed to read guest tools runner metadata")?;
    if !metadata.socket_path.exists() {
        return Ok(());
    }

    // Connect host-first and HOLD the connection. The guest agent emits its
    // `GuestHello` once, as the first frame on the channel, when it boots ~a
    // minute in. If we reconnected on every tick we would usually attach AFTER
    // that one-shot hello had already flushed and instead read a later frame
    // (its periodic Heartbeat), which `read_guest_session` rejects as
    // `ExpectedGuestHello`. Connecting once up front (well before the agent is
    // up) guarantees the hello reaches us as the first frame on this held
    // reader. This mirrors how the live opt-in harness connects before
    // launching the agent.
    if backend.guest_tools_pending.is_none() {
        let stream = match UnixStream::connect(&metadata.socket_path) {
            Ok(stream) => stream,
            Err(error)
                if matches!(
                    error.kind(),
                    ErrorKind::NotFound | ErrorKind::ConnectionRefused | ErrorKind::WouldBlock
                ) =>
            {
                return Ok(());
            }
            Err(error) => {
                return Err(error).with_context(|| {
                    format!(
                        "failed to connect guest tools socket {}",
                        metadata.socket_path.display()
                    )
                });
            }
        };
        stream
            .set_read_timeout(Some(Duration::from_millis(25)))
            .context("failed to configure guest tools read timeout")?;
        backend.guest_tools_pending = Some(stream);
    }

    let policy = guest_tools_agent_policy(store, name).map_err(anyhow::Error::msg)?;
    let stream = backend
        .guest_tools_pending
        .as_mut()
        .expect("guest tools pending stream present");

    // Peek (MSG_PEEK) for a COMPLETE newline-terminated frame before consuming
    // anything. This makes the held connection resumable: the agent's one-shot
    // GuestHello can be split across host reads (virtio-serial chunks it), and a
    // plain `read_line` over the 25ms-timeout socket would consume a partial
    // frame, lose those bytes when the timeout fires mid-frame, then fail to
    // parse the tail and reset -- permanently missing the (already-flushed)
    // hello. Only consuming once the whole line is present means a mid-frame
    // timeout can never drop bytes.
    let mut peek = [0u8; 16384];
    // `UnixStream::peek` is unstable on stable Rust, so peek via libc recv(2)
    // with MSG_PEEK. SAFETY: `fd` is a valid open socket owned by `stream` for
    // the duration of the call, and `peek` is a valid writable buffer of
    // `peek.len()` bytes. The socket's SO_RCVTIMEO (the read timeout set above)
    // applies, so this returns EAGAIN rather than blocking when no data is ready.
    let peeked = unsafe {
        libc::recv(
            stream.as_raw_fd(),
            peek.as_mut_ptr() as *mut libc::c_void,
            peek.len(),
            libc::MSG_PEEK,
        )
    };
    let peeked = if peeked > 0 {
        peeked as usize
    } else if peeked == 0 {
        // EOF: the socket closed (VM/QEMU gone). Drop + reconnect next tick.
        backend.guest_tools_pending = None;
        return Ok(());
    } else {
        let error = std::io::Error::last_os_error();
        if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) {
            // No data yet (agent still booting). Keep the held connection.
            return Ok(());
        }
        return Err(error).context("failed to peek guest tools socket");
    };
    let Some(newline) = peek[..peeked].iter().position(|&byte| byte == b'\n') else {
        if peeked == peek.len() {
            // A frame fills the entire peek window with no newline: oversized or
            // malformed (a well-formed GuestHello is far smaller). Waiting would
            // spin forever since the newline can never appear inside the window.
            // Reset so the next tick reconnects host-first.
            eprintln!(
                "bridgevmd resetting guest-tools session for '{name}': oversized handshake frame"
            );
            backend.guest_tools_pending = None;
            return Ok(());
        }
        // Only a partial frame is buffered so far -- leave it unconsumed and
        // wait for the rest on a later tick.
        return Ok(());
    };

    // A whole line is present, so consuming exactly it cannot time out mid-frame.
    let mut frame = vec![0u8; newline + 1];
    stream
        .read_exact(&mut frame)
        .context("failed to read guest hello frame")?;
    let frame = String::from_utf8_lossy(&frame);
    let session = decode_envelope_line(&frame)
        .map_err(AgentSessionIoError::from)
        .and_then(|envelope| {
            accept_guest_hello(&envelope, &policy).map_err(AgentSessionIoError::from)
        });
    match session {
        Ok(session) => {
            write_guest_tools_runtime(store, name, &session, GuestToolsRuntimeUpdate::Connected)?;
            let stream = backend
                .guest_tools_pending
                .take()
                .expect("guest tools pending stream present after accept");
            backend.guest_tools = Some(session);
            // Bytes the agent sent right after the hello (its initial Heartbeat +
            // status burst) are still in the kernel socket buffer; the drain
            // reader picks them up.
            backend.guest_tools_stream = Some(BufReader::new(stream));
        }
        // The first frame was not a valid GuestHello -> reset and reconnect.
        Err(error) => {
            eprintln!("bridgevmd resetting guest-tools session for '{name}': {error:?}");
            backend.guest_tools_pending = None;
        }
    }
    Ok(())
}

pub(crate) fn drain_guest_tools_messages(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
) -> Result<()> {
    let Some(session) = backend.guest_tools.clone() else {
        return Ok(());
    };
    for _ in 0..GUEST_TOOLS_DRAIN_LIMIT {
        let envelope = {
            let Some(reader) = backend.guest_tools_stream.as_mut() else {
                return Ok(());
            };
            match read_envelope_line(reader) {
                Ok(Some(envelope)) => envelope,
                Ok(None) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    return Ok(());
                }
                Err(error) if error.is_idle_io() => return Ok(()),
                Err(error) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("failed to read guest tools frame: {error:?}");
                }
            }
        };

        process_guest_tools_envelope(store, name, backend, &session, envelope)?;
    }

    Ok(())
}

pub(crate) const GUEST_TOOLS_DRAIN_LIMIT: usize = 16;

pub(crate) const GUEST_TOOLS_COMMAND_RESULT_TIMEOUT: Duration = Duration::from_secs(5);

impl DaemonState {
    pub(crate) fn send_guest_tools_command(
        &mut self,
        name: &str,
        envelope: AgentEnvelope,
    ) -> Result<BridgeVmResponse> {
        Ok(BridgeVmResponse::GuestToolsCommand {
            command: self.send_guest_tools_command_record(name, envelope)?,
        })
    }

    pub(crate) fn send_guest_tools_command_record(
        &mut self,
        name: &str,
        envelope: AgentEnvelope,
    ) -> Result<GuestToolsCommandRecord> {
        let backend = self
            .children
            .get_mut(name)
            .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
        let session = backend
            .guest_tools
            .as_ref()
            .with_context(|| format!("guest tools session is not connected for '{name}'"))?;
        let stream = backend
            .guest_tools_stream
            .as_mut()
            .with_context(|| format!("guest tools stream is not connected for '{name}'"))?;

        backend
            .guest_tools_commands
            .begin_host_command(session, &envelope)
            .map_err(|error| anyhow::anyhow!("guest tools command rejected: {error:?}"))?;
        write_envelope_line(stream.get_mut(), &envelope)
            .map_err(|error| anyhow::anyhow!("failed to write guest tools command: {error:?}"))?;

        Ok(GuestToolsCommandRecord {
            vm: name.to_string(),
            request_id: envelope.request_id,
            pending_commands: backend.guest_tools_commands.pending_count(),
        })
    }

    pub(crate) fn wait_for_guest_tools_command_result(
        &mut self,
        name: &str,
        request_id: &str,
        timeout: Duration,
    ) -> Result<CompletedGuestToolsCommand> {
        let deadline = Instant::now() + timeout;
        loop {
            let backend = self
                .children
                .get_mut(name)
                .with_context(|| format!("backend is not owned by this daemon for '{name}'"))?;
            let session = backend
                .guest_tools
                .clone()
                .with_context(|| format!("guest tools session is not connected for '{name}'"))?;
            let Some(reader) = backend.guest_tools_stream.as_mut() else {
                anyhow::bail!("guest tools stream is not connected for '{name}'");
            };

            let envelope = match read_envelope_line(reader) {
                Ok(Some(envelope)) => envelope,
                Ok(None) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("guest tools stream closed while waiting for '{request_id}'");
                }
                Err(error) if error.is_idle_io() => {
                    if Instant::now() >= deadline {
                        anyhow::bail!(
                            "timed out waiting for guest tools command result '{request_id}'"
                        );
                    }
                    thread::sleep(Duration::from_millis(10));
                    continue;
                }
                Err(error) => {
                    backend.guest_tools = None;
                    backend.guest_tools_stream = None;
                    anyhow::bail!("failed to read guest tools frame: {error:?}");
                }
            };

            if let Some(completed) =
                process_guest_tools_envelope(&self.store, name, backend, &session, envelope)?
            {
                if completed.request_id == request_id {
                    return Ok(completed);
                }
            }

            if Instant::now() >= deadline {
                anyhow::bail!("timed out waiting for guest tools command result '{request_id}'");
            }
        }
    }
}
