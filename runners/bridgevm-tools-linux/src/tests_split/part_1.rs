//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::time::Duration;

#[test]
fn hello_advertises_core_linux_capabilities() {
    let envelope = guest_hello("token-1", "linux", default_capabilities());
    envelope.validate().unwrap();

    let AgentMessage::GuestHello {
        guest_os,
        capabilities,
        auth,
        ..
    } = envelope.message
    else {
        panic!("expected GuestHello");
    };

    assert_eq!(guest_os, "linux");
    assert_eq!(
        auth,
        Some(AgentAuth::ToolsToken {
            token: "token-1".to_string()
        })
    );
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "clipboard"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "heartbeat"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "display-resize"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "drag-drop"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "applications"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "windows"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "fs-freeze"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "fs-thaw"));
    assert!(capabilities
        .iter()
        .any(|capability| capability.name == "agent-update"));
}

#[test]
fn request_id_commands_get_command_results() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello".to_string(),
        },
        "clipboard-1",
    );

    let mut state = GuestToolsState::new(&default_capabilities());
    let result = state.handle_command(&command).unwrap();
    assert_eq!(
        result.message,
        AgentMessage::CommandResult {
            request_id: "clipboard-1".to_string(),
            ok: true,
            error_code: None,
            message: None,
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn clipboard_command_backend_writes_text_and_reports_success() {
    let root = unique_temp_dir("bridgevm-tools-clipboard");
    fs::create_dir_all(&root).unwrap();
    let command_path = root.join("write-clipboard.sh");
    let output_path = root.join("clipboard.txt");
    fs::write(
        &command_path,
        format!("#!/bin/sh\ncat > '{}'\n", output_path.display()),
    )
    .unwrap();
    let mut permissions = fs::metadata(&command_path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&command_path, permissions).unwrap();

    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello from host".to_string(),
        },
        "clipboard-1",
    );
    let mut state = GuestToolsState::new(&default_capabilities())
        .with_clipboard_writer(ClipboardWriter::command(command_path));

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "clipboard-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("clipboard updated".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(fs::read_to_string(output_path).unwrap(), "hello from host");
}

#[test]
fn clipboard_command_backend_reports_failures() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello".to_string(),
        },
        "clipboard-1",
    );
    let mut state = GuestToolsState::new(&default_capabilities()).with_clipboard_writer(
        ClipboardWriter::command(PathBuf::from("/tmp/bridgevm-missing-clipboard-command")),
    );

    let result = state.handle_command(&command).unwrap();
    let AgentMessage::CommandResult {
        request_id,
        ok,
        error_code,
        message,
        ..
    } = result.message
    else {
        panic!("expected CommandResult");
    };
    assert_eq!(request_id, "clipboard-1");
    assert!(!ok);
    assert_eq!(error_code.as_deref(), Some("clipboard-write-failed"));
    assert!(message
        .as_deref()
        .unwrap()
        .contains("failed to execute clipboard command"));
}

#[test]
fn clipboard_commands_require_capability() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello".to_string(),
        },
        "clipboard-1",
    );
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "clipboard-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("clipboard capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn display_resize_command_backend_passes_dimensions_and_reports_success() {
    let root = unique_temp_dir("bridgevm-tools-display-resize");
    fs::create_dir_all(&root).unwrap();
    let command_path = root.join("resize-display.sh");
    let output_path = root.join("resize.txt");
    fs::write(
        &command_path,
        format!(
            "#!/bin/sh\nprintf '%s %s %s' \"$1\" \"$2\" \"$3\" > '{}'\n",
            output_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&command_path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&command_path, permissions).unwrap();

    let command = AgentEnvelope::with_request_id(
        AgentMessage::ResizeDisplay {
            width: 1440,
            height: 900,
            scale: 2,
        },
        "resize-1",
    );
    let mut state = GuestToolsState::new(&default_capabilities())
        .with_display_resizer(DisplayResizer::command(command_path));

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "resize-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("display resized to 1440x900 scale 2".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(fs::read_to_string(output_path).unwrap(), "1440 900 2");
}

#[test]
fn display_resize_command_backend_reports_failures() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::ResizeDisplay {
            width: 1440,
            height: 900,
            scale: 2,
        },
        "resize-1",
    );
    let mut state = GuestToolsState::new(&default_capabilities()).with_display_resizer(
        DisplayResizer::command(PathBuf::from("/tmp/bridgevm-missing-display-command")),
    );

    let result = state.handle_command(&command).unwrap();
    let AgentMessage::CommandResult {
        request_id,
        ok,
        error_code,
        message,
        ..
    } = result.message
    else {
        panic!("expected CommandResult");
    };
    assert_eq!(request_id, "resize-1");
    assert!(!ok);
    assert_eq!(error_code.as_deref(), Some("display-resize-failed"));
    assert!(message
        .as_deref()
        .unwrap()
        .contains("failed to execute display resize command"));
}

#[test]
fn display_resize_commands_require_capability() {
    let command = AgentEnvelope::with_request_id(
        AgentMessage::ResizeDisplay {
            width: 1440,
            height: 900,
            scale: 2,
        },
        "resize-1",
    );
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "resize-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("display resize capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn clipboard_detection_prefers_wayland_wl_copy() {
    // Wayland session with both tools available -> wl-copy, no args, even
    // though xclip is also present.
    let env = FakeDesktopEnv {
        envs: &["WAYLAND_DISPLAY", "DISPLAY"],
        programs: &["wl-copy", "xclip"],
    };
    let writer = detect_clipboard_writer(&env);
    let (program, args) = writer.command_for_test().expect("expected a command");
    assert_eq!(program, Path::new("wl-copy"));
    assert!(args.is_empty());
}

#[test]
fn clipboard_detection_falls_back_to_x11_xclip() {
    // X11 session (no WAYLAND_DISPLAY) -> xclip with selection args.
    let env = FakeDesktopEnv {
        envs: &["DISPLAY"],
        programs: &["xclip"],
    };
    let writer = detect_clipboard_writer(&env);
    let (program, args) = writer.command_for_test().expect("expected a command");
    assert_eq!(program, Path::new("xclip"));
    assert_eq!(args, &["-selection".to_string(), "clipboard".to_string()]);
}

#[test]
fn clipboard_detection_falls_back_to_simulated_without_tools() {
    // Wayland var set but wl-copy missing, and no DISPLAY -> simulated.
    let env = FakeDesktopEnv {
        envs: &["WAYLAND_DISPLAY"],
        programs: &[],
    };
    assert!(detect_clipboard_writer(&env).command_for_test().is_none());

    // No session at all -> simulated.
    let env = FakeDesktopEnv {
        envs: &[],
        programs: &["wl-copy", "xclip"],
    };
    assert!(detect_clipboard_writer(&env).command_for_test().is_none());
}

#[test]
fn clipboard_reader_detection_prefers_wayland_wl_paste() {
    // Wayland session with both tools -> wl-paste --no-newline.
    let env = FakeDesktopEnv {
        envs: &["WAYLAND_DISPLAY", "DISPLAY"],
        programs: &["wl-paste", "xclip"],
    };
    let reader = detect_clipboard_reader(&env);
    let (program, args) = reader.command_for_test().expect("expected a command");
    assert_eq!(program, Path::new("wl-paste"));
    assert_eq!(args, &["--no-newline".to_string()]);
}

#[test]
fn clipboard_reader_detection_falls_back_to_x11_xclip() {
    // X11 session (no WAYLAND_DISPLAY) -> xclip -selection clipboard -o.
    let env = FakeDesktopEnv {
        envs: &["DISPLAY"],
        programs: &["xclip"],
    };
    let reader = detect_clipboard_reader(&env);
    let (program, args) = reader.command_for_test().expect("expected a command");
    assert_eq!(program, Path::new("xclip"));
    assert_eq!(
        args,
        &[
            "-selection".to_string(),
            "clipboard".to_string(),
            "-o".to_string(),
        ]
    );
}

#[test]
fn clipboard_reader_detection_falls_back_to_simulated_without_tools() {
    // Wayland var set but wl-paste missing, and no DISPLAY -> simulated.
    let env = FakeDesktopEnv {
        envs: &["WAYLAND_DISPLAY"],
        programs: &[],
    };
    assert!(detect_clipboard_reader(&env).command_for_test().is_none());

    // No session at all -> simulated, even though both tools exist.
    let env = FakeDesktopEnv {
        envs: &[],
        programs: &["wl-paste", "xclip"],
    };
    assert!(detect_clipboard_reader(&env).command_for_test().is_none());
}

#[test]
fn clipboard_watch_state_emits_on_first_non_empty_and_dedupes() {
    let mut state = ClipboardWatchState::new();
    // First non-empty read is emitted.
    assert_eq!(
        state.observe(Some("hello".to_string())),
        Some("hello".to_string())
    );
    // Identical repeats are suppressed.
    assert_eq!(state.observe(Some("hello".to_string())), None);
    assert_eq!(state.observe(Some("hello".to_string())), None);
    // A real change re-emits.
    assert_eq!(
        state.observe(Some("world".to_string())),
        Some("world".to_string())
    );
    assert_eq!(state.observe(Some("world".to_string())), None);
}

#[test]
fn clipboard_watch_state_ignores_empty_and_none_without_clearing() {
    let mut state = ClipboardWatchState::new();
    // Empty/None before any value: nothing to report.
    assert_eq!(state.observe(None), None);
    assert_eq!(state.observe(Some(String::new())), None);

    assert_eq!(
        state.observe(Some("payload".to_string())),
        Some("payload".to_string())
    );
    // A transient empty/None read does not clear the remembered value, so
    // the same text afterwards is still a dedup (no spurious re-emit).
    assert_eq!(state.observe(None), None);
    assert_eq!(state.observe(Some(String::new())), None);
    assert_eq!(state.observe(Some("payload".to_string())), None);
    // But a genuinely new value after the gap is emitted.
    assert_eq!(
        state.observe(Some("next".to_string())),
        Some("next".to_string())
    );
}

#[test]
fn clipboard_reader_simulated_value_round_trips_through_watch_state() {
    let reader = ClipboardReader::simulated_value(Some("seed".to_string()));
    let mut state = ClipboardWatchState::new();
    assert_eq!(
        state.observe(reader.read_text().unwrap()),
        Some("seed".to_string())
    );
    // Same simulated value again -> dedup.
    assert_eq!(state.observe(reader.read_text().unwrap()), None);
}

#[test]
fn output_drain_caps_capture_but_consumes_reader_to_eof() {
    let input = vec![0x5a; OUTPUT_DRAIN_BUFFER_BYTES * 3];
    let mut reader = std::io::Cursor::new(input.clone());

    let (captured, exceeded) =
        drain_output_bounded(&mut reader, OUTPUT_DRAIN_BUFFER_BYTES + 17).unwrap();

    assert!(exceeded);
    assert_eq!(captured, input[..OUTPUT_DRAIN_BUFFER_BYTES + 17]);
    assert_eq!(reader.position(), input.len() as u64);
}

#[test]
fn output_drain_preserves_complete_bounded_output() {
    let input = b"bounded output".to_vec();
    let mut reader = std::io::Cursor::new(input.clone());

    let (captured, exceeded) = drain_output_bounded(&mut reader, input.len()).unwrap();

    assert!(!exceeded);
    assert_eq!(captured, input);
}

#[test]
fn bounded_wait_terminates_and_reaps_timed_out_process() {
    let mut child = ProcessCommand::new("/bin/sh")
        .args(["-c", "sleep 5"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();

    let error =
        wait_bounded_for(&mut child, "test command", Duration::from_millis(20)).unwrap_err();

    assert_eq!(error, "test command timed out");
    assert!(child.try_wait().unwrap().is_some());
}

#[test]
fn bounded_utf8_reader_rejects_sparse_oversized_file_before_allocation() {
    let root = unique_temp_dir("bridgevm-tools-bounded-text");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("oversized.txt");
    let file = fs::File::create(&path).unwrap();
    file.set_len(512 * 1024 * 1024).unwrap();

    let error = read_utf8_file_bounded(&path, 4096).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("536870912 bytes"));
    assert!(error.to_string().contains("4096 byte limit"));
}

#[test]
fn bounded_utf8_reader_rejects_invalid_utf8() {
    let root = unique_temp_dir("bridgevm-tools-invalid-text");
    fs::create_dir_all(&root).unwrap();
    let path = root.join("invalid.txt");
    fs::write(&path, [0xff, 0xfe]).unwrap();

    let error = read_utf8_file_bounded(&path, 4096).unwrap_err();

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("not valid UTF-8"));
}

#[test]
fn clipboard_reader_command_captures_stdout_and_trims_trailing_newline() {
    let root = unique_temp_dir("bridgevm-tools-clipboard-read");
    fs::create_dir_all(&root).unwrap();
    let command_path = root.join("read-clipboard.sh");
    fs::write(&command_path, "#!/bin/sh\nprintf 'clip text\\n'\n").unwrap();
    let mut permissions = fs::metadata(&command_path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&command_path, permissions).unwrap();

    let reader = ClipboardReader::command(command_path);
    assert_eq!(reader.read_text().unwrap(), Some("clip text".to_string()));
}

#[test]
fn clipboard_reader_command_treats_empty_output_as_none() {
    let root = unique_temp_dir("bridgevm-tools-clipboard-read-empty");
    fs::create_dir_all(&root).unwrap();
    let command_path = root.join("read-empty.sh");
    fs::write(&command_path, "#!/bin/sh\nprintf ''\n").unwrap();
    let mut permissions = fs::metadata(&command_path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&command_path, permissions).unwrap();

    let reader = ClipboardReader::command(command_path);
    assert_eq!(reader.read_text().unwrap(), None);
}

#[test]
fn clipboard_reader_command_reports_failure() {
    let reader = ClipboardReader::command(PathBuf::from("/tmp/bridgevm-missing-clipboard-reader"));
    assert!(reader.read_text().is_err());
}

#[test]
fn resolve_clipboard_watcher_uses_explicit_read_command() {
    let watcher = resolve_clipboard_watcher(
        &default_capabilities(),
        250,
        Some(PathBuf::from("/usr/local/bin/my-reader")),
    )
    .unwrap()
    .expect("expected an enabled watcher");
    assert_eq!(watcher.interval, Duration::from_millis(250));
    assert_eq!(
        watcher.reader.command_path(),
        Some(Path::new("/usr/local/bin/my-reader"))
    );
}
