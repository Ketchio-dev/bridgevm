//! Split out of main.rs to keep files under 800 lines.

use crate::*;
use anyhow::Context;
use anyhow::Result;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agentd::accept_guest_hello;
use bridgevm_agentd::authorize_message;
use bridgevm_agentd::decode_envelope_line;
use bridgevm_agentd::read_envelope_line;
use bridgevm_agentd::AgentSession;
use bridgevm_agentd::AgentSessionIoError;
use bridgevm_api::guest_tools_agent_policy;
use bridgevm_api::ApplicationConsistentSnapshotCommandResultRecord;
use bridgevm_api::PerformanceMeasurementRecord;
use bridgevm_api::PerformanceSampleMetadata;
use bridgevm_qemu::query_status;
use bridgevm_qemu::QemuError;
use bridgevm_qemu::QmpClient;
use bridgevm_qemu::QmpEventDrain;
use bridgevm_storage::GuestToolsIpAddressMetadata;
use bridgevm_storage::QmpSupervisorMetadata;
use bridgevm_storage::VmStore;
use std::collections::HashMap;
use std::fs;
use std::io::BufReader;
use std::io::ErrorKind;
use std::io::Read;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::time::Duration;
use std::time::SystemTime;

pub(crate) fn connect_supervisor_qmp(socket_path: &Path) -> Result<QmpClient, QemuError> {
    let mut client = QmpClient::connect_with_timeout(socket_path, Duration::from_millis(25))?;
    client.negotiate()?;
    Ok(client)
}

pub(crate) fn compatibility_qemu_command_error(error: QemuError) -> String {
    format!("failed to build Compatibility Mode QEMU command: {error}")
}

pub(crate) struct QmpSupervisorReport {
    pub(crate) terminal: bool,
    pub(crate) drain: Option<QmpEventDrain>,
}

pub(crate) fn qmp_supervisor_report(
    client: &mut Option<QmpClient>,
    socket_path: &Path,
) -> QmpSupervisorReport {
    let Some(client_ref) = client.as_mut() else {
        return QmpSupervisorReport {
            terminal: qmp_status_is_terminal(socket_path),
            drain: None,
        };
    };

    match client_ref.drain_events(QMP_SUPERVISOR_DRAIN_LIMIT) {
        Ok(drain) => {
            let terminal = drain.has_terminal_event();
            let should_record =
                drain.envelopes_read > 0 || drain.limit_reached || drain.terminal_event.is_some();
            QmpSupervisorReport {
                terminal,
                drain: should_record.then_some(drain),
            }
        }
        Err(error) if error.is_qmp_idle() => QmpSupervisorReport {
            terminal: false,
            drain: None,
        },
        Err(_) => {
            *client = None;
            QmpSupervisorReport {
                terminal: qmp_status_is_terminal(socket_path),
                drain: None,
            }
        }
    }
}

pub(crate) fn qmp_status_is_terminal(socket_path: &Path) -> bool {
    query_status(socket_path)
        .map(|status| status.is_terminal())
        .unwrap_or(false)
}

pub(crate) fn command_result_timeout(freeze_timeout_millis: Option<u64>) -> Duration {
    freeze_timeout_millis
        .map(|millis| Duration::from_millis(millis).saturating_add(Duration::from_secs(1)))
        .unwrap_or(GUEST_TOOLS_COMMAND_RESULT_TIMEOUT)
}

pub(crate) fn write_qmp_supervisor_metadata(
    store: &VmStore,
    name: &str,
    drain: &QmpEventDrain,
) -> Result<()> {
    store
        .write_qmp_supervisor_metadata(
            name,
            &QmpSupervisorMetadata {
                events: drain.events.clone(),
                terminal_event: drain.terminal_event.clone(),
                envelopes_read: drain.envelopes_read,
                limit_reached: drain.limit_reached,
                updated_at_unix: now_unix(),
            },
        )
        .context("failed to write QMP supervisor metadata")
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CompletedGuestToolsCommand {
    pub(crate) request_id: String,
    pub(crate) capability: Option<String>,
    pub(crate) ok: bool,
    pub(crate) error_code: Option<String>,
    pub(crate) message: Option<String>,
    pub(crate) result: Option<serde_json::Value>,
    pub(crate) metadata: Option<serde_json::Value>,
    pub(crate) completed_at_unix: u64,
    pub(crate) pending_commands: usize,
}

impl CompletedGuestToolsCommand {
    pub(crate) fn into_record(self) -> ApplicationConsistentSnapshotCommandResultRecord {
        ApplicationConsistentSnapshotCommandResultRecord {
            request_id: self.request_id,
            capability: self.capability,
            ok: self.ok,
            error_code: self.error_code,
            message: self.message,
            completed_at_unix: self.completed_at_unix,
        }
    }
}

pub(crate) fn record_guest_benchmark_result(
    sample: &mut PerformanceSampleMetadata,
    completed: &CompletedGuestToolsCommand,
) {
    sample
        .notes
        .retain(|note| note != "host-side sample; no guest benchmark workloads were executed");
    if !completed.ok {
        let reason = completed
            .error_code
            .as_deref()
            .or(completed.message.as_deref())
            .unwrap_or("command-result-not-ok");
        sample.notes.push(format!(
            "guest benchmark command did not produce measurements: {reason}"
        ));
        return;
    }

    sample.notes.push(format!(
        "guest benchmark executed over daemon-owned guest-tools session (request id {})",
        completed.request_id
    ));
    let Some(result) = completed.result.as_ref() else {
        sample
            .notes
            .push("guest benchmark completed without a result payload".to_string());
        return;
    };

    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/budget_duration_millis",
        "guest_benchmark_budget_millis",
        "milliseconds",
        "guest_tools.benchmark.budget_duration_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/iterations",
        "guest_benchmark_cpu_iterations",
        "count",
        "guest_tools.benchmark.cpu.iterations",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/elapsed_millis",
        "guest_benchmark_cpu_elapsed_millis",
        "milliseconds",
        "guest_tools.benchmark.cpu.elapsed_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/cpu/ops_per_sec",
        "guest_benchmark_cpu_ops_per_sec",
        "ops_per_second",
        "guest_tools.benchmark.cpu.ops_per_sec",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/bytes_written",
        "guest_benchmark_disk_bytes_written",
        "bytes",
        "guest_tools.benchmark.disk.bytes_written",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/elapsed_millis",
        "guest_benchmark_disk_elapsed_millis",
        "milliseconds",
        "guest_tools.benchmark.disk.elapsed_millis",
    );
    push_guest_benchmark_measurement(
        &mut sample.measurements,
        result,
        "/disk/mib_per_sec",
        "guest_benchmark_disk_mib_per_sec",
        "MiB_per_second",
        "guest_tools.benchmark.disk.mib_per_sec",
    );
    if let Some(error) = result.get("disk_error").and_then(|value| value.as_str()) {
        sample.notes.push(format!(
            "guest benchmark disk micro-benchmark skipped: {error}"
        ));
    }
}

pub(crate) fn push_guest_benchmark_measurement(
    measurements: &mut Vec<PerformanceMeasurementRecord>,
    result: &serde_json::Value,
    pointer: &str,
    name: &str,
    unit: &str,
    source: &str,
) {
    if let Some(value) = result.pointer(pointer).and_then(|value| value.as_u64()) {
        measurements.push(PerformanceMeasurementRecord {
            name: name.to_string(),
            value,
            unit: unit.to_string(),
            source: source.to_string(),
            metadata_only: false,
        });
    }
}

pub(crate) fn process_guest_tools_envelope(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
    session: &AgentSession,
    envelope: AgentEnvelope,
) -> Result<Option<CompletedGuestToolsCommand>> {
    authorize_message(session, &envelope.message)
        .map_err(|error| anyhow::anyhow!("unauthorized guest tools message: {error:?}"))?;
    match &envelope.message {
        AgentMessage::CommandResult {
            request_id,
            ok,
            error_code,
            message,
            result,
            metadata,
        } => {
            let pending = backend
                .guest_tools_commands
                .complete_command_result(&envelope)
                .map_err(|error| {
                    anyhow::anyhow!("unexpected guest tools command result: {error:?}")
                })?;
            let completed_at_unix = now_unix();
            let mut result = result.clone();
            let metadata = metadata.clone();
            if *ok && pending.capability.as_deref() == Some("windows") {
                if let Err(message) =
                    attach_proxy_window_crop_artifacts(store, name, backend, result.as_mut())
                {
                    eprintln!("bridgevmd: proxy window crop artifact skipped: {message}");
                }
            }
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::CommandResult {
                    request_id: request_id.clone(),
                    capability: pending.capability.clone(),
                    ok: *ok,
                    error_code: error_code.clone(),
                    message: message.clone(),
                    result: result.clone(),
                    metadata: metadata.clone(),
                    completed_at_unix,
                },
            )?;
            if *ok {
                match pending.message {
                    AgentMessage::MountShare {
                        name: share_name,
                        host_path_token,
                    } => {
                        write_guest_tools_runtime(
                            store,
                            name,
                            session,
                            GuestToolsRuntimeUpdate::MountShare {
                                name: share_name,
                                host_path_token,
                            },
                        )?;
                    }
                    AgentMessage::UnmountShare { name: share_name } => {
                        write_guest_tools_runtime(
                            store,
                            name,
                            session,
                            GuestToolsRuntimeUpdate::UnmountShare { name: share_name },
                        )?;
                    }
                    _ => {}
                }
            }
            Ok(Some(CompletedGuestToolsCommand {
                request_id: request_id.clone(),
                capability: pending.capability,
                ok: *ok,
                error_code: error_code.clone(),
                message: message.clone(),
                result,
                metadata,
                completed_at_unix,
                pending_commands: backend.guest_tools_commands.pending_count(),
            }))
        }
        AgentMessage::Heartbeat => {
            write_guest_tools_runtime(store, name, session, GuestToolsRuntimeUpdate::Heartbeat)?;
            Ok(None)
        }
        AgentMessage::GuestIpChanged { addresses } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::GuestIp(
                    addresses
                        .iter()
                        .map(|address| GuestToolsIpAddressMetadata {
                            address: address.address.to_string(),
                            interface: address.interface.clone(),
                        })
                        .collect(),
                ),
            )?;
            Ok(None)
        }
        AgentMessage::GuestMetrics {
            cpu_percent,
            memory_used_mib,
        } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::Metrics {
                    cpu_percent: *cpu_percent,
                    memory_used_mib: *memory_used_mib,
                },
            )?;
            Ok(None)
        }
        AgentMessage::AgentUpdateAvailable {
            current_version,
            available_version,
            download_url,
            signature,
        } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::AgentUpdateAvailable {
                    current_version: current_version.clone(),
                    available_version: available_version.clone(),
                    download_url: download_url.clone(),
                    signature: signature.clone(),
                },
            )?;
            Ok(None)
        }
        AgentMessage::ClipboardChanged { text } => {
            write_guest_tools_runtime(
                store,
                name,
                session,
                GuestToolsRuntimeUpdate::Clipboard { text: text.clone() },
            )?;
            Ok(None)
        }
        _ => Ok(None),
    }
}

pub(crate) struct ProxyWindowCropConfig {
    pub(crate) artifact_dir: PathBuf,
    pub(crate) framebuffer_rgba_file: PathBuf,
    pub(crate) framebuffer_width: u32,
    pub(crate) framebuffer_height: u32,
    pub(crate) backing_scale: u16,
}

#[derive(Debug, Clone)]
pub(crate) struct ProxyWindowCropTarget {
    pub(crate) id: String,
    pub(crate) title: Option<String>,
    pub(crate) x: i32,
    pub(crate) y: i32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone)]
pub(crate) struct ProxyWindowClippedRect {
    pub(crate) x: u32,
    pub(crate) y: u32,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProxyWindowFramebufferSignature {
    pub(crate) path: PathBuf,
    pub(crate) len: u64,
    pub(crate) modified: Option<SystemTime>,
}

pub(crate) fn attach_proxy_window_crop_artifacts(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
    result: Option<&mut serde_json::Value>,
) -> Result<(), String> {
    let Some(config) = ProxyWindowCropConfig::from_env(store, name)? else {
        return Ok(());
    };
    let Some(result) = result else {
        return Ok(());
    };
    let Some(payload) = result.as_object_mut() else {
        return Ok(());
    };

    if let Some(serde_json::Value::Array(windows)) = payload.get_mut("windows") {
        let mut targets = HashMap::new();
        for window in windows {
            if let Some(target) = attach_proxy_window_crop_artifact(&config, window)? {
                targets.insert(target.id.clone(), target);
            }
        }
        backend.proxy_window_crop_targets = targets;
    }
    if let Some(window) = payload.get_mut("window") {
        if let Some(closed_id) = proxy_window_closed_id(window) {
            backend.proxy_window_crop_targets.remove(&closed_id);
        } else if let Some(target) = attach_proxy_window_crop_artifact(&config, window)? {
            backend
                .proxy_window_crop_targets
                .insert(target.id.clone(), target);
        }
    }
    backend.proxy_window_framebuffer_signature = Some(proxy_window_framebuffer_signature(&config)?);

    Ok(())
}

pub(crate) fn attach_proxy_window_crop_artifact(
    config: &ProxyWindowCropConfig,
    window: &mut serde_json::Value,
) -> Result<Option<ProxyWindowCropTarget>, String> {
    let Some(target) = proxy_window_crop_target(window) else {
        return Ok(None);
    };
    let Some(summary_path) = materialize_proxy_window_crop_target(config, &target)? else {
        return Ok(None);
    };

    if let Some(map) = window.as_object_mut() {
        map.insert(
            "window_crop_frame_summary_path".to_string(),
            serde_json::Value::String(summary_path.display().to_string()),
        );
    }

    Ok(Some(target))
}

pub(crate) fn refresh_proxy_window_crop_artifacts(
    store: &VmStore,
    name: &str,
    backend: &mut SupervisedBackend,
) -> Result<(), String> {
    if backend.proxy_window_crop_targets.is_empty() {
        return Ok(());
    }
    let Some(config) = ProxyWindowCropConfig::from_env(store, name)? else {
        return Ok(());
    };
    let signature = proxy_window_framebuffer_signature(&config)?;
    if backend.proxy_window_framebuffer_signature.as_ref() == Some(&signature) {
        return Ok(());
    }

    for target in backend.proxy_window_crop_targets.values() {
        materialize_proxy_window_crop_target(&config, target)?;
    }
    backend.proxy_window_framebuffer_signature = Some(signature);
    Ok(())
}

pub(crate) fn materialize_proxy_window_crop_target(
    config: &ProxyWindowCropConfig,
    target: &ProxyWindowCropTarget,
) -> Result<Option<PathBuf>, String> {
    let Some(clipped) =
        clip_proxy_window_crop_target(target, config.framebuffer_width, config.framebuffer_height)
    else {
        return Ok(None);
    };

    let slug = safe_proxy_window_artifact_slug(&target.id);
    let summary_path = config.artifact_dir.join(format!("{slug}.json"));
    let rgba_path = config.artifact_dir.join(format!("{slug}.rgba"));
    fs::create_dir_all(&config.artifact_dir).map_err(|error| {
        format!(
            "failed to create proxy window artifact directory {}: {error}",
            config.artifact_dir.display()
        )
    })?;
    materialize_proxy_window_crop(config, &clipped, &rgba_path)?;
    write_proxy_window_crop_summary(config, target, &clipped, &summary_path, &rgba_path)?;

    Ok(Some(summary_path))
}

impl ProxyWindowCropConfig {
    pub(crate) fn from_env(store: &VmStore, name: &str) -> Result<Option<Self>, String> {
        if let Some(framebuffer_rgba_file) =
            std::env::var_os("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE")
        {
            let framebuffer_rgba_file = PathBuf::from(framebuffer_rgba_file);
            let framebuffer_width = required_u32_env("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_WIDTH")?;
            let framebuffer_height = required_u32_env("BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_HEIGHT")?;
            return Self::from_parts(
                store,
                name,
                framebuffer_rgba_file,
                framebuffer_width,
                framebuffer_height,
            );
        }
        Self::from_runner_metadata(store, name)
    }

    pub(crate) fn from_runner_metadata(
        store: &VmStore,
        name: &str,
    ) -> Result<Option<Self>, String> {
        let Some(metadata) = store
            .runner_metadata(name)
            .map_err(|error| format!("failed to read runner metadata for {name}: {error}"))?
        else {
            return Ok(None);
        };
        if !metadata
            .command
            .iter()
            .any(|arg| arg == "--apple-vz-display")
        {
            return Ok(None);
        }
        let Some(framebuffer_rgba_file) =
            runner_arg_path(&metadata.command, "--apple-vz-proxy-framebuffer-rgba-file")
        else {
            return Ok(None);
        };
        if !framebuffer_rgba_file.is_file() {
            return Ok(None);
        }
        let framebuffer_width =
            runner_arg_u32(&metadata.command, "--apple-vz-display-width")?.unwrap_or(1280);
        let framebuffer_height =
            runner_arg_u32(&metadata.command, "--apple-vz-display-height")?.unwrap_or(800);
        Self::from_parts(
            store,
            name,
            framebuffer_rgba_file,
            framebuffer_width,
            framebuffer_height,
        )
    }

    pub(crate) fn from_parts(
        store: &VmStore,
        name: &str,
        framebuffer_rgba_file: PathBuf,
        framebuffer_width: u32,
        framebuffer_height: u32,
    ) -> Result<Option<Self>, String> {
        let backing_scale = optional_u16_env("BRIDGEVM_PROXY_WINDOW_BACKING_SCALE")?.unwrap_or(1);
        let artifact_dir = match std::env::var_os("BRIDGEVM_PROXY_WINDOW_ARTIFACT_DIR") {
            Some(path) => PathBuf::from(path),
            None => {
                let (bundle, _) = store
                    .get_vm(name)
                    .map_err(|error| format!("failed to resolve VM bundle for {name}: {error}"))?;
                bundle.join("metadata").join("proxy-windows")
            }
        };

        Ok(Some(Self {
            artifact_dir,
            framebuffer_rgba_file,
            framebuffer_width,
            framebuffer_height,
            backing_scale: backing_scale.max(1),
        }))
    }
}

pub(crate) fn runner_arg_path(command: &[String], flag: &str) -> Option<PathBuf> {
    runner_arg_value(command, flag).map(PathBuf::from)
}

pub(crate) fn runner_arg_u32(command: &[String], flag: &str) -> Result<Option<u32>, String> {
    let Some(value) = runner_arg_value(command, flag) else {
        return Ok(None);
    };
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .map(Some)
        .ok_or_else(|| format!("{flag} in runner metadata must be a positive u32, got '{value}'"))
}

pub(crate) fn runner_arg_value(command: &[String], flag: &str) -> Option<String> {
    command
        .windows(2)
        .find(|pair| pair[0] == flag)
        .map(|pair| pair[1].clone())
}

pub(crate) fn required_u32_env(name: &str) -> Result<u32, String> {
    let value = std::env::var(name).map_err(|_| {
        format!("{name} must be set when BRIDGEVM_PROXY_WINDOW_FRAMEBUFFER_RGBA_FILE is set")
    })?;
    value
        .parse::<u32>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or_else(|| format!("{name} must be a positive u32, got '{value}'"))
}
