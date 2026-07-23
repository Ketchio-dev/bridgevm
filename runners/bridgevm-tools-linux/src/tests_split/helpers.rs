//! Split test module.

use crate::*;
use anyhow::Result;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agentd::decode_envelope_line;
use std::fs;
use std::io::BufRead;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use std::time::SystemTime;
use std::time::UNIX_EPOCH;

/// Fake desktop environment for auto-detection tests: records which env vars
/// are "set" and which programs are "on PATH" without touching the real
/// process environment or running xclip/xrandr.
pub(super) struct FakeDesktopEnv {
    pub(super) envs: &'static [&'static str],
    pub(super) programs: &'static [&'static str],
}

impl DesktopEnv for FakeDesktopEnv {
    fn has_env(&self, name: &str) -> bool {
        self.envs.contains(&name)
    }

    fn has_program(&self, program: &str) -> bool {
        self.programs.contains(&program)
    }

    fn program_path(&self, program: &str) -> Option<PathBuf> {
        self.has_program(program).then(|| PathBuf::from(program))
    }
}

#[test]
fn resolve_clipboard_watcher_disabled_by_default() {
    // Interval 0 (default) -> no watcher, default behavior unchanged.
    assert!(resolve_clipboard_watcher(&default_capabilities(), 0, None)
        .unwrap()
        .is_none());
}

#[test]
fn resolve_clipboard_watcher_rejects_read_command_without_interval() {
    assert!(resolve_clipboard_watcher(
        &default_capabilities(),
        0,
        Some(PathBuf::from("/usr/local/bin/my-reader")),
    )
    .is_err());
}

#[test]
fn resolve_clipboard_watcher_requires_capability() {
    let heartbeat_only = [AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }];
    assert!(resolve_clipboard_watcher(&heartbeat_only, 250, None).is_err());
}

#[test]
fn watched_session_emits_clipboard_changed_on_real_change() {
    // A reader script whose output changes on the second read; the watcher
    // must emit one ClipboardChanged frame for each distinct value.
    let root = unique_temp_dir("bridgevm-tools-clipboard-watch");
    fs::create_dir_all(&root).unwrap();
    let value_path = root.join("clip-value.txt");
    fs::write(&value_path, "first").unwrap();
    let command_path = root.join("read-clip.sh");
    fs::write(
        &command_path,
        format!("#!/bin/sh\ncat '{}'\n", value_path.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&command_path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&command_path, permissions).unwrap();

    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let value_path_for_server = value_path.clone();
    let server = thread::spawn(move || {
        let (stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());

        // Drain hello + initial status frames.
        let hello = read_frame(&mut reader);
        assert!(matches!(hello.message, AgentMessage::GuestHello { .. }));
        assert_eq!(read_frame(&mut reader).message, AgentMessage::Heartbeat);
        assert!(matches!(
            read_frame(&mut reader).message,
            AgentMessage::GuestIpChanged { .. }
        ));
        assert!(matches!(
            read_frame(&mut reader).message,
            AgentMessage::GuestMetrics { .. }
        ));

        // First watcher emission: "first".
        assert_eq!(
            read_frame(&mut reader).message,
            AgentMessage::ClipboardChanged {
                text: "first".to_string()
            }
        );
        // Change the clipboard value; the watcher must re-emit.
        fs::write(&value_path_for_server, "second").unwrap();
        assert_eq!(
            read_frame(&mut reader).message,
            AgentMessage::ClipboardChanged {
                text: "second".to_string()
            }
        );
        drop(reader);
        drop(stream);
    });

    let stream = UnixStream::connect(&socket_path).unwrap();
    let writer = stream.try_clone().unwrap();
    let watcher = ClipboardWatcher {
        interval: Duration::from_millis(10),
        reader: ClipboardReader::command(command_path),
    };
    run_tools_session_watched(stream, writer, default_session_config(false), Some(watcher))
        .unwrap();

    server.join().unwrap();
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn fire_and_forget_share_commands_update_state_without_result() {
    let mut state = GuestToolsState::new(&default_capabilities());
    let mount = AgentEnvelope::new(AgentMessage::MountShare {
        name: "workspace".to_string(),
        host_path_token: "host-token-1".to_string(),
    });

    assert_eq!(state.handle_command(&mount), None);
    assert!(state.shared_folders.contains_key("workspace"));
}

pub(super) struct RecordingClockBackend {
    pub(super) applied: std::rc::Rc<std::cell::RefCell<Vec<u64>>>,
    pub(super) fail: bool,
}

impl ClockBackend for RecordingClockBackend {
    fn set_epoch_millis(&mut self, unix_epoch_millis: u64) -> Result<(), String> {
        if self.fail {
            return Err("injected settimeofday failure".to_string());
        }
        self.applied.borrow_mut().push(unix_epoch_millis);
        Ok(())
    }
}

pub(super) struct RecordingFreezeBackend {
    pub(super) calls: std::rc::Rc<std::cell::RefCell<Vec<String>>>,
    pub(super) fail_freeze: Option<PathBuf>,
}

impl FilesystemFreezeBackend for RecordingFreezeBackend {
    fn freeze(&mut self, mount: &Path, _timeout_millis: Option<u64>) -> Result<(), String> {
        self.calls
            .borrow_mut()
            .push(format!("freeze:{}", mount.display()));
        if self.fail_freeze.as_deref() == Some(mount) {
            Err("injected freeze failure".to_string())
        } else {
            Ok(())
        }
    }

    fn thaw(&mut self, mount: &Path) -> Result<(), String> {
        self.calls
            .borrow_mut()
            .push(format!("thaw:{}", mount.display()));
        Ok(())
    }
}

pub(super) fn read_frame(reader: &mut impl BufRead) -> AgentEnvelope {
    let mut line = String::new();
    reader.read_line(&mut line).unwrap();
    decode_envelope_line(&line).unwrap()
}

pub(super) fn temp_socket_path() -> PathBuf {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    std::env::temp_dir().join(format!("bvmt-{}-{micros}.sock", std::process::id()))
}

pub(super) fn unique_temp_dir(prefix: &str) -> PathBuf {
    let micros = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_micros();
    std::env::temp_dir().join(format!("{prefix}-{}-{micros}", std::process::id()))
}

pub(super) fn default_telemetry() -> TelemetryConfig {
    // Force synthetic metrics so the frame count is deterministic on any
    // host (real /proc reads would vary the values, not the count, but we
    // keep tests host-independent).
    TelemetryConfig::from_args(
        &default_capabilities(),
        &[],
        false,
        1,
        256,
        false,
        true,
        None,
    )
    .unwrap()
}

pub(super) fn default_session_config(serve_once: bool) -> ToolsSessionConfig<'static> {
    ToolsSessionConfig {
        token: "token-1",
        guest_os: "linux",
        capabilities: default_capabilities(),
        // No clipboard seed: watcher tests can distinguish live updates
        // from startup telemetry without changing the production path.
        telemetry: default_telemetry(),
        file_drop_dir: None,
        filesystem_freezer: FilesystemFreezer::simulated(),
        clipboard_writer: ClipboardWriter::simulated(),
        display_resizer: DisplayResizer::simulated(),
        clock_setter: ClockSetter::simulated(),
        desktop_controller: DesktopController::simulated(),
        serve_once,
    }
}
