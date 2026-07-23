//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use clap::Parser;
use std::path::PathBuf;

#[test]
fn file_drop_commands_can_write_payload_to_configured_directory() {
    let root = unique_temp_dir("bridgevm-tools-file-drop");
    let mut state =
        GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root.clone()));
    let start = AgentEnvelope::with_request_id(
        AgentMessage::FileDropStart {
            transfer_id: "drop-1".to_string(),
            file_name: "notes.txt".to_string(),
            size_bytes: 11,
        },
        "drop-start-1",
    );
    let chunk = AgentEnvelope::with_request_id(
        AgentMessage::FileDropChunk {
            transfer_id: "drop-1".to_string(),
            chunk_index: 0,
            data_base64: "aGVsbG8gd29ybGQ=".to_string(),
        },
        "drop-chunk-1",
    );
    let complete = AgentEnvelope::with_request_id(
        AgentMessage::FileDropComplete {
            transfer_id: "drop-1".to_string(),
        },
        "drop-complete-1",
    );

    assert!(matches!(
        state.handle_command(&start).unwrap().message,
        AgentMessage::CommandResult { ok: true, .. }
    ));
    assert!(matches!(
        state.handle_command(&chunk).unwrap().message,
        AgentMessage::CommandResult { ok: true, .. }
    ));
    let result = state.handle_command(&complete).unwrap();

    assert_eq!(
        result.message,
        AgentMessage::CommandResult {
            request_id: "drop-complete-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(format!(
                "completed file drop notes.txt (11 bytes across 1 chunks) at {}",
                root.join("notes.txt").display()
            )),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        std::fs::read_to_string(root.join("notes.txt")).unwrap(),
        "hello world"
    );
    assert!(state.file_drops.is_empty());
}

#[test]
fn file_drop_write_rejects_unsafe_names_and_size_mismatches() {
    let root = unique_temp_dir("bridgevm-tools-file-drop-errors");
    let mut state =
        GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root.clone()));
    let start = AgentEnvelope::with_request_id(
        AgentMessage::FileDropStart {
            transfer_id: "drop-1".to_string(),
            file_name: "../notes.txt".to_string(),
            size_bytes: 11,
        },
        "drop-start-1",
    );
    let chunk = AgentEnvelope::with_request_id(
        AgentMessage::FileDropChunk {
            transfer_id: "drop-1".to_string(),
            chunk_index: 0,
            data_base64: "aGVsbG8gd29ybGQ=".to_string(),
        },
        "drop-chunk-1",
    );
    let complete = AgentEnvelope::with_request_id(
        AgentMessage::FileDropComplete {
            transfer_id: "drop-1".to_string(),
        },
        "drop-complete-1",
    );

    state.handle_command(&start).unwrap();
    state.handle_command(&chunk).unwrap();
    assert_eq!(
        state.handle_command(&complete).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-complete-1".to_string(),
            ok: false,
            error_code: Some("unsafe-file-name".to_string()),
            message: Some("file drop file name is not safe: ../notes.txt".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert!(!root.join("notes.txt").exists());

    let mut state = GuestToolsState::new(&default_capabilities()).with_file_drop_dir(Some(root));
    let start = AgentEnvelope::with_request_id(
        AgentMessage::FileDropStart {
            transfer_id: "drop-2".to_string(),
            file_name: "short.txt".to_string(),
            size_bytes: 12,
        },
        "drop-start-2",
    );
    let chunk = AgentEnvelope::with_request_id(
        AgentMessage::FileDropChunk {
            transfer_id: "drop-2".to_string(),
            chunk_index: 0,
            data_base64: "aGVsbG8gd29ybGQ=".to_string(),
        },
        "drop-chunk-2",
    );
    let complete = AgentEnvelope::with_request_id(
        AgentMessage::FileDropComplete {
            transfer_id: "drop-2".to_string(),
        },
        "drop-complete-2",
    );

    state.handle_command(&start).unwrap();
    state.handle_command(&chunk).unwrap();
    assert_eq!(
        state.handle_command(&complete).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-complete-2".to_string(),
            ok: false,
            error_code: Some("transfer-size-mismatch".to_string()),
            message: Some("file drop short.txt expected 12 bytes but received 11".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn file_drop_commands_require_capability_and_start_order() {
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);
    let start = AgentEnvelope::with_request_id(
        AgentMessage::FileDropStart {
            transfer_id: "drop-1".to_string(),
            file_name: "notes.txt".to_string(),
            size_bytes: 11,
        },
        "drop-start-1",
    );

    assert_eq!(
        state.handle_command(&start).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-start-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("drag-and-drop capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );

    let mut state = GuestToolsState::new(&default_capabilities());
    let complete = AgentEnvelope::with_request_id(
        AgentMessage::FileDropComplete {
            transfer_id: "missing".to_string(),
        },
        "drop-complete-1",
    );

    assert_eq!(
        state.handle_command(&complete).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-complete-1".to_string(),
            ok: false,
            error_code: Some("transfer-not-started".to_string()),
            message: Some("file drop missing has not started".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn application_commands_track_alpha_state() {
    let mut state = GuestToolsState::new(&default_capabilities());

    let list = AgentEnvelope::with_request_id(AgentMessage::ListApplications, "apps-1");
    assert_eq!(
        state.handle_command(&list).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "apps-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(
                "applications: org.bridgevm.files:Files,org.bridgevm.terminal:Terminal".to_string()
            ),
            result: Some(serde_json::json!({
                "applications": [
                    {
                        "id": "org.bridgevm.files",
                        "name": "Files",
                        "launched": false
                    },
                    {
                        "id": "org.bridgevm.terminal",
                        "name": "Terminal",
                        "launched": false
                    }
                ]
            })),
            metadata: None,
        }
    );

    let launch = AgentEnvelope::with_request_id(
        AgentMessage::LaunchApplication {
            id: "org.bridgevm.terminal".to_string(),
        },
        "launch-1",
    );
    assert_eq!(
        state.handle_command(&launch).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "launch-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted launch request for application Terminal".to_string()),
            result: Some(serde_json::json!({
                "application": {
                    "id": "org.bridgevm.terminal",
                    "name": "Terminal",
                    "launched": true
                }
            })),
            metadata: None,
        }
    );
    assert!(state
        .applications
        .get("org.bridgevm.terminal")
        .is_some_and(|app| app.launched));
}

#[test]
fn window_commands_track_alpha_state() {
    let mut state = GuestToolsState::new(&default_capabilities());

    let list = AgentEnvelope::with_request_id(AgentMessage::ListWindows, "windows-1");
    assert_eq!(
        state.handle_command(&list).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "windows-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("windows: window-1:BridgeVM Linux Desktop".to_string()),
            result: Some(serde_json::json!({
                "windows": [
                    {
                        "id": "window-1",
                        "title": "BridgeVM Linux Desktop",
                        "focused": true
                    }
                ]
            })),
            metadata: None,
        }
    );

    let focus = AgentEnvelope::with_request_id(
        AgentMessage::FocusWindow {
            id: "window-1".to_string(),
        },
        "focus-1",
    );
    assert_eq!(
        state.handle_command(&focus).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "focus-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted focus request for window BridgeVM Linux Desktop".to_string()),
            result: Some(serde_json::json!({
                "window": {
                    "id": "window-1",
                    "title": "BridgeVM Linux Desktop",
                    "focused": true
                }
            })),
            metadata: None,
        }
    );

    let bounds = AgentEnvelope::with_request_id(
        AgentMessage::SetWindowBounds {
            id: "window-1".to_string(),
            x: 30,
            y: 40,
            width: 800,
            height: 600,
        },
        "bounds-1",
    );
    assert_eq!(
        state.handle_command(&bounds).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "bounds-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("set bounds for window BridgeVM Linux Desktop".to_string()),
            result: Some(serde_json::json!({
                "window": {
                    "id": "window-1",
                    "title": "BridgeVM Linux Desktop",
                    "bounds": {
                        "x": 30,
                        "y": 40,
                        "width": 800,
                        "height": 600
                    },
                    "bounds_changed": true
                }
            })),
            metadata: None,
        }
    );
    assert_eq!(
        state
            .windows
            .get("window-1")
            .and_then(|window| window.bounds.clone()),
        Some(DesktopWindowBounds {
            x: 30,
            y: 40,
            width: 800,
            height: 600,
        })
    );

    let close = AgentEnvelope::with_request_id(
        AgentMessage::CloseWindow {
            id: "window-1".to_string(),
        },
        "close-1",
    );
    assert_eq!(
        state.handle_command(&close).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "close-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("closed window BridgeVM Linux Desktop".to_string()),
            result: Some(serde_json::json!({
                "window": {
                    "id": "window-1",
                    "title": "BridgeVM Linux Desktop",
                    "bounds": {
                        "x": 30,
                        "y": 40,
                        "width": 800,
                        "height": 600
                    },
                    "closed": true
                }
            })),
            metadata: None,
        }
    );
    assert!(state
        .windows
        .get("window-1")
        .is_some_and(|window| window.closed && !window.focused));
}

#[test]
fn application_and_window_commands_require_capabilities() {
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);

    let launch = AgentEnvelope::with_request_id(
        AgentMessage::LaunchApplication {
            id: "org.bridgevm.terminal".to_string(),
        },
        "launch-1",
    );
    assert_eq!(
        state.handle_command(&launch).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "launch-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("applications capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );

    let focus = AgentEnvelope::with_request_id(
        AgentMessage::FocusWindow {
            id: "window-1".to_string(),
        },
        "focus-1",
    );
    assert_eq!(
        state.handle_command(&focus).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "focus-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("windows capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );

    let bounds = AgentEnvelope::with_request_id(
        AgentMessage::SetWindowBounds {
            id: "window-1".to_string(),
            x: 30,
            y: 40,
            width: 800,
            height: 600,
        },
        "bounds-1",
    );
    assert_eq!(
        state.handle_command(&bounds).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "bounds-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("windows capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn filesystem_freeze_thaw_tracks_scaffold_boundary() {
    let mut state = GuestToolsState::new(&default_capabilities());
    let freeze = AgentEnvelope::with_request_id(
        AgentMessage::FreezeFilesystem {
            timeout_millis: Some(30_000),
        },
        "freeze-1",
    );
    let duplicate_freeze = AgentEnvelope::with_request_id(
        AgentMessage::FreezeFilesystem {
            timeout_millis: None,
        },
        "freeze-2",
    );
    let thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-1");
    let duplicate_thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-2");

    assert_eq!(
        state.handle_command(&freeze).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "freeze-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(
                "entered simulated filesystem freeze scaffold boundary with timeout 30000 ms; no OS fsfreeze was executed and application consistency is not guaranteed"
                    .to_string()
            ),
            result: None,
            metadata: None,
        }
    );
    assert!(state.filesystem_frozen);
    assert_eq!(
        state.handle_command(&duplicate_freeze).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "freeze-2".to_string(),
            ok: false,
            error_code: Some("filesystem-already-frozen".to_string()),
            message: Some("filesystem freeze scaffold boundary is already active".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state.handle_command(&thaw).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "thaw-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(
                "left simulated filesystem thaw scaffold boundary; no OS fsfreeze was executed and application consistency is not guaranteed"
                    .to_string()
            ),
            result: None,
            metadata: None,
        }
    );
    assert!(!state.filesystem_frozen);
    assert_eq!(
        state.handle_command(&duplicate_thaw).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "thaw-2".to_string(),
            ok: false,
            error_code: Some("filesystem-not-frozen".to_string()),
            message: Some("filesystem thaw scaffold boundary is not active".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn filesystem_freeze_thaw_require_capabilities() {
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);
    let freeze = AgentEnvelope::with_request_id(
        AgentMessage::FreezeFilesystem {
            timeout_millis: None,
        },
        "freeze-1",
    );
    assert_eq!(
        state.handle_command(&freeze).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "freeze-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("filesystem freeze capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );

    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "fs-freeze".to_string(),
        version: 1,
    }]);
    let thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-1");
    assert_eq!(
        state.handle_command(&thaw).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "thaw-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("filesystem thaw capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn real_filesystem_freezer_uses_allowlisted_mounts_and_reverse_thaw() {
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let backend = RecordingFreezeBackend {
        calls: calls.clone(),
        fail_freeze: None,
    };
    let mut state = GuestToolsState::new(&default_capabilities()).with_filesystem_freezer(
        FilesystemFreezer::real(
            vec![PathBuf::from("/"), PathBuf::from("/var")],
            Box::new(backend),
        ),
    );

    let freeze = AgentEnvelope::with_request_id(
        AgentMessage::FreezeFilesystem {
            timeout_millis: Some(10_000),
        },
        "freeze-1",
    );
    let thaw = AgentEnvelope::with_request_id(AgentMessage::ThawFilesystem, "thaw-1");

    let result = state.handle_command(&freeze).unwrap();
    assert_eq!(
        result.message,
        AgentMessage::CommandResult {
            request_id: "freeze-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(
                "entered real fsfreeze boundary for /, /var; application consistency still depends on guest applications flushing state"
                    .to_string()
            ),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(calls.borrow().as_slice(), ["freeze:/", "freeze:/var"]);

    let result = state.handle_command(&thaw).unwrap();
    assert_eq!(
            result.message,
            AgentMessage::CommandResult {
                request_id: "thaw-1".to_string(),
            ok: true,
            error_code: None,
            message: Some(
                "left real fsfreeze boundary for /, /var; application consistency still depends on guest applications flushing state"
                    .to_string()
            ),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        calls.borrow().as_slice(),
        ["freeze:/", "freeze:/var", "thaw:/var", "thaw:/"]
    );
}

#[test]
fn real_filesystem_freezer_rolls_back_on_partial_freeze_failure() {
    let calls = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let backend = RecordingFreezeBackend {
        calls: calls.clone(),
        fail_freeze: Some(PathBuf::from("/var")),
    };
    let mut state = GuestToolsState::new(&default_capabilities()).with_filesystem_freezer(
        FilesystemFreezer::real(
            vec![PathBuf::from("/"), PathBuf::from("/var")],
            Box::new(backend),
        ),
    );
    let freeze = AgentEnvelope::with_request_id(
        AgentMessage::FreezeFilesystem {
            timeout_millis: Some(10_000),
        },
        "freeze-1",
    );

    let result = state.handle_command(&freeze).unwrap();
    assert_eq!(
        result.message,
        AgentMessage::CommandResult {
            request_id: "freeze-1".to_string(),
            ok: false,
            error_code: Some("filesystem-freeze-failed".to_string()),
            message: Some(
                "failed to freeze /var: injected freeze failure; rollback thaw succeeded"
                    .to_string()
            ),
            result: None,
            metadata: None,
        }
    );
    assert!(!state.filesystem_frozen);
    assert_eq!(
        calls.borrow().as_slice(),
        ["freeze:/", "freeze:/var", "thaw:/"]
    );
}

#[test]
fn real_filesystem_freezer_args_require_explicit_absolute_mounts() {
    assert!(resolve_filesystem_freezer(false, vec![PathBuf::from("/")]).is_err());
    assert!(resolve_filesystem_freezer(true, Vec::new()).is_err());
    assert!(resolve_filesystem_freezer(true, vec![PathBuf::from("relative")]).is_err());
    assert!(resolve_filesystem_freezer(
        true,
        vec![PathBuf::from("/var"), PathBuf::from("/var/."),]
    )
    .is_err());
    assert_eq!(
        normalize_fsfreeze_mounts(vec![PathBuf::from("/var/../var/lib")]).unwrap(),
        vec![PathBuf::from("/var/lib")]
    );
}

#[test]
fn real_fsfreeze_cli_parses_opt_in_mount_flags() {
    let args = Args::try_parse_from([
        "bridgevm-tools-linux",
        "--socket",
        "tools.sock",
        "--token",
        "token-1",
        "--real-fsfreeze",
        "--fsfreeze-mount",
        "/",
        "--fsfreeze-mount",
        "/var",
    ])
    .unwrap();

    assert!(args.real_fsfreeze);
    assert_eq!(
        args.fsfreeze_mounts,
        vec![PathBuf::from("/"), PathBuf::from("/var")]
    );
}

#[test]
fn time_sync_with_real_backend_applies_epoch_and_replies_with_result() {
    let applied = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let backend = RecordingClockBackend {
        applied: applied.clone(),
        fail: false,
    };
    let mut state = GuestToolsState::new(&default_capabilities())
        .with_clock_setter(ClockSetter::real(Box::new(backend)));

    let command = AgentEnvelope::with_request_id(
        AgentMessage::TimeSync {
            unix_epoch_millis: 1_781_470_123_456,
        },
        "time-1",
    );

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "time-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("set guest clock to 1781470123456 ms since epoch".to_string()),
            result: Some(serde_json::json!({
                "applied_unix_epoch_millis": 1_781_470_123_456u64,
            })),
            metadata: None,
        }
    );
    assert_eq!(applied.borrow().as_slice(), [1_781_470_123_456]);
}
