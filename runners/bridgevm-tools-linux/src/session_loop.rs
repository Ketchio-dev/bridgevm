//! The agent session run loop: handshake, command loop, and the optional clipboard-watcher thread.

use crate::*;
use anyhow::Result;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agentd::read_envelope_line;
use bridgevm_agentd::write_envelope_line;
use std::io::BufReader;
use std::io::Read;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::sync::Mutex;
use std::thread;
use std::time::Duration;

pub(crate) struct ToolsSessionConfig<'a> {
    pub(crate) token: &'a str,
    pub(crate) guest_os: &'a str,
    pub(crate) capabilities: Vec<AgentCapability>,
    pub(crate) telemetry: TelemetryConfig,
    pub(crate) file_drop_dir: Option<PathBuf>,
    pub(crate) filesystem_freezer: FilesystemFreezer,
    pub(crate) clipboard_writer: ClipboardWriter,
    pub(crate) display_resizer: DisplayResizer,
    pub(crate) clock_setter: ClockSetter,
    pub(crate) desktop_controller: DesktopController,
    pub(crate) serve_once: bool,
}

pub(crate) fn run_tools_session(
    reader: impl Read,
    writer: &mut impl Write,
    config: ToolsSessionConfig<'_>,
) -> Result<()> {
    let ToolsSessionConfig {
        token,
        guest_os,
        capabilities,
        telemetry,
        file_drop_dir,
        filesystem_freezer,
        clipboard_writer,
        display_resizer,
        clock_setter,
        desktop_controller,
        serve_once,
    } = config;
    let mut state = GuestToolsState::new(&capabilities)
        .with_file_drop_dir(file_drop_dir)
        .with_filesystem_freezer(filesystem_freezer)
        .with_clipboard_writer(clipboard_writer)
        .with_display_resizer(display_resizer)
        .with_clock_setter(clock_setter)
        .with_desktop_controller(desktop_controller);
    let hello = guest_hello(token, guest_os, capabilities);
    write_envelope_line(writer, &hello).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    for envelope in initial_status_envelopes(&telemetry) {
        write_envelope_line(writer, &envelope).map_err(|error| anyhow::anyhow!("{error:?}"))?;
    }

    let mut reader = BufReader::new(reader);
    let mut handled_commands = 0usize;
    while let Some(command) =
        read_envelope_line(&mut reader).map_err(|error| anyhow::anyhow!("{error:?}"))?
    {
        if let Some(result) = state.handle_command(&command) {
            write_envelope_line(writer, &result).map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
        handled_commands += 1;
        if serve_once && handled_commands >= 1 {
            break;
        }
    }

    Ok(())
}

/// Session runner with the opt-in continuous clipboard watcher layered on top
/// of the proven `run_tools_session` loop. When `watcher` is `None` (the
/// default, interval 0) this is byte-for-byte the same as `run_tools_session`:
/// no extra thread is spawned and the writer is used directly. When a watcher
/// is supplied, the writer is shared behind a `Mutex` so the watcher thread and
/// the command loop never interleave frames, the watcher polls the real reader
/// on its interval and emits `ClipboardChanged` through the same writer, and the
/// watcher is signalled to stop and joined before this function returns.
pub(crate) fn run_tools_session_watched<R, W>(
    reader: R,
    mut writer: W,
    config: ToolsSessionConfig<'_>,
    clipboard_watcher: Option<ClipboardWatcher>,
) -> Result<()>
where
    R: Read,
    W: Write + Send + 'static,
{
    let Some(watcher) = clipboard_watcher else {
        // Disabled watcher: identical to the historical single-threaded path.
        return run_tools_session(reader, &mut writer, config);
    };

    // Share the writer so the watcher thread and the command loop serialize
    // their frames. write_envelope_line flushes per frame, so a frame written
    // under the lock is complete before the lock is released.
    let shared_writer = Arc::new(Mutex::new(writer));
    let stop = Arc::new(AtomicBool::new(false));
    let watcher_handle =
        spawn_clipboard_watcher(watcher, Arc::clone(&shared_writer), Arc::clone(&stop));

    let result = run_tools_session_shared(reader, &shared_writer, config);

    // Signal the watcher to stop and reap it so no thread leaks and no frame is
    // written after the session loop has returned.
    stop.store(true, Ordering::SeqCst);
    let _ = watcher_handle.join();
    result
}

/// Spawn the watcher thread. It polls the reader every `interval`, feeds reads
/// through the pure `ClipboardWatchState`, and writes a `ClipboardChanged`
/// frame under the shared writer lock on each detected change. It exits when
/// `stop` is set or when writing to the shared writer fails (session gone).
pub(crate) fn spawn_clipboard_watcher<W>(
    watcher: ClipboardWatcher,
    shared_writer: Arc<Mutex<W>>,
    stop: Arc<AtomicBool>,
) -> thread::JoinHandle<()>
where
    W: Write + Send + 'static,
{
    thread::spawn(move || {
        let ClipboardWatcher { interval, reader } = watcher;
        let mut state = ClipboardWatchState::new();
        // Poll in short slices so a long interval still stops promptly.
        let slice = interval
            .min(Duration::from_millis(100))
            .max(Duration::from_millis(1));
        let mut waited = Duration::ZERO;
        loop {
            if stop.load(Ordering::SeqCst) {
                return;
            }
            if waited < interval {
                thread::sleep(slice);
                waited += slice;
                continue;
            }
            waited = Duration::ZERO;

            // A reader error is non-fatal: skip this tick and try again. A
            // misbehaving reader is already bounded by run_clipboard_read_command.
            let latest = reader.read_text().unwrap_or(None);
            if let Some(text) = state.observe(latest) {
                let envelope = AgentEnvelope::new(AgentMessage::ClipboardChanged { text });
                let Ok(mut guard) = shared_writer.lock() else {
                    return;
                };
                if write_envelope_line(&mut *guard, &envelope).is_err() {
                    // Stream closed / session ended: stop watching.
                    return;
                }
            }
        }
    })
}

/// The command loop body, parameterized over a shared writer so the watcher can
/// safely interleave `ClipboardChanged` frames. Mirrors `run_tools_session`.
pub(crate) fn run_tools_session_shared<R, W>(
    reader: R,
    shared_writer: &Arc<Mutex<W>>,
    config: ToolsSessionConfig<'_>,
) -> Result<()>
where
    R: Read,
    W: Write,
{
    let ToolsSessionConfig {
        token,
        guest_os,
        capabilities,
        telemetry,
        file_drop_dir,
        filesystem_freezer,
        clipboard_writer,
        display_resizer,
        clock_setter,
        desktop_controller,
        serve_once,
    } = config;
    let mut state = GuestToolsState::new(&capabilities)
        .with_file_drop_dir(file_drop_dir)
        .with_filesystem_freezer(filesystem_freezer)
        .with_clipboard_writer(clipboard_writer)
        .with_display_resizer(display_resizer)
        .with_clock_setter(clock_setter)
        .with_desktop_controller(desktop_controller);
    let hello = guest_hello(token, guest_os, capabilities);
    {
        let mut guard = shared_writer.lock().expect("writer mutex poisoned");
        write_envelope_line(&mut *guard, &hello).map_err(|error| anyhow::anyhow!("{error:?}"))?;
        for envelope in initial_status_envelopes(&telemetry) {
            write_envelope_line(&mut *guard, &envelope)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
    }

    let mut reader = BufReader::new(reader);
    let mut handled_commands = 0usize;
    while let Some(command) =
        read_envelope_line(&mut reader).map_err(|error| anyhow::anyhow!("{error:?}"))?
    {
        if let Some(result) = state.handle_command(&command) {
            let mut guard = shared_writer.lock().expect("writer mutex poisoned");
            write_envelope_line(&mut *guard, &result)
                .map_err(|error| anyhow::anyhow!("{error:?}"))?;
        }
        handled_commands += 1;
        if serve_once && handled_commands >= 1 {
            break;
        }
    }

    Ok(())
}
