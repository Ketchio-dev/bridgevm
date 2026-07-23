//! Split test module.

use super::helpers::*;
use crate::*;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::WindowInputEvent;
use bridgevm_agentd::write_envelope_line;
use std::fs;
use std::io::BufReader;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::net::UnixListener;
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::path::PathBuf;
use std::thread;

#[test]
fn watched_session_without_watcher_matches_plain_session() {
    // None watcher must behave exactly like run_tools_session: hello +
    // initial status + a CommandResult, and no ClipboardChanged (no seed,
    // no watcher). Uses a socket so the owned writer is Send + 'static.
    let socket_path = temp_socket_path();
    let listener = UnixListener::bind(&socket_path).unwrap();
    let server = thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        let mut reader = BufReader::new(stream.try_clone().unwrap());

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

        let command = AgentEnvelope::with_request_id(
            AgentMessage::ResizeDisplay {
                width: 800,
                height: 600,
                scale: 1,
            },
            "resize-1",
        );
        write_envelope_line(&mut stream, &command).unwrap();

        let result = read_frame(&mut reader);
        assert!(matches!(
            result.message,
            AgentMessage::CommandResult { ok: true, .. }
        ));
    });

    let stream = UnixStream::connect(&socket_path).unwrap();
    let writer = stream.try_clone().unwrap();
    run_tools_session_watched(stream, writer, default_session_config(true), None).unwrap();

    server.join().unwrap();
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn display_resize_detection_uses_xrandr_on_x11() {
    let env = FakeDesktopEnv {
        envs: &["DISPLAY"],
        programs: &["xrandr"],
    };
    let resizer = detect_display_resizer(&env);
    assert_eq!(
        resizer.command_for_test().expect("expected a command"),
        Path::new("xrandr")
    );

    // Missing DISPLAY or missing xrandr -> simulated.
    let no_display = FakeDesktopEnv {
        envs: &[],
        programs: &["xrandr"],
    };
    assert!(detect_display_resizer(&no_display)
        .command_for_test()
        .is_none());
    let no_tool = FakeDesktopEnv {
        envs: &["DISPLAY"],
        programs: &[],
    };
    assert!(detect_display_resizer(&no_tool)
        .command_for_test()
        .is_none());
}

#[test]
fn desktop_controller_detection_uses_real_tools_when_available() {
    let env = FakeDesktopEnv {
        envs: &["DISPLAY"],
        programs: &["gio", "wmctrl"],
    };
    assert!(detect_desktop_controller(&env, true, true).is_real_for_test());

    let no_tools = FakeDesktopEnv {
        envs: &["DISPLAY"],
        programs: &[],
    };
    assert!(!detect_desktop_controller(&no_tools, true, true).is_real_for_test());
}

#[test]
fn desktop_file_parser_filters_visible_applications() {
    let root = unique_temp_dir("bridgevm-tools-desktop-file");
    fs::create_dir_all(&root).unwrap();
    let terminal = root.join("org.bridgevm.Terminal.desktop");
    fs::write(
        &terminal,
        "[Desktop Entry]\nType=Application\nName=Terminal\nExec=terminal\n",
    )
    .unwrap();
    let app = parse_desktop_application(&terminal).expect("visible desktop app");
    assert_eq!(app.id, "org.bridgevm.Terminal.desktop");
    assert_eq!(app.name, "Terminal");

    let hidden = root.join("hidden.desktop");
    fs::write(
        &hidden,
        "[Desktop Entry]\nType=Application\nName=Hidden\nNoDisplay=true\n",
    )
    .unwrap();
    assert!(parse_desktop_application(&hidden).is_none());
}

#[test]
fn wmctrl_window_backend_lists_focuses_and_closes_real_tool_output() {
    let root = unique_temp_dir("bridgevm-tools-wmctrl");
    fs::create_dir_all(&root).unwrap();
    let command_path = root.join("wmctrl");
    let action_path = root.join("action.txt");
    fs::write(
        &command_path,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  -l) if [ \"$2\" = \"-p\" ] && [ \"$3\" = \"-G\" ]; then echo '0x01200007  0 4242 30 40 800 600 guest Terminal'; else echo '0x01200007  0 guest Terminal'; fi;;\n  -ia|-ic) printf '%s %s' \"$1\" \"$2\" > '{}';;\n  -ir) printf '%s %s %s %s' \"$1\" \"$2\" \"$3\" \"$4\" > '{}';;\n  *) exit 2;;\nesac\n",
            action_path.display(),
            action_path.display()
        ),
    )
    .unwrap();
    let mut permissions = fs::metadata(&command_path).unwrap().permissions();
    permissions.set_mode(0o700);
    fs::set_permissions(&command_path, permissions).unwrap();

    let mut controller = DesktopController::real(None, Some(command_path), None);
    let list = controller.list_windows().expect("real window backend");
    assert!(list.ok);
    assert_eq!(
        list.result,
        Some(serde_json::json!({
            "windows": [
                {
                    "id": "0x01200007",
                    "title": "Terminal",
                    "desktop": 0,
                    "pid": 4242,
                    "bounds": {
                        "x": 30,
                        "y": 40,
                        "width": 800,
                        "height": 600
                    },
                    "focused": false,
                    "source": "wmctrl"
                }
            ]
        }))
    );

    let focus = controller
        .focus_window("0x01200007")
        .expect("real focus backend");
    assert!(focus.ok);
    assert_eq!(fs::read_to_string(&action_path).unwrap(), "-ia 0x01200007");

    let bounds = controller
        .set_window_bounds("0x01200007", 50, 60, 1024, 768)
        .expect("real bounds backend");
    assert!(bounds.ok);
    assert_eq!(
        bounds.result,
        Some(serde_json::json!({
            "window": {
                "id": "0x01200007",
                "title": "Terminal",
                "desktop": 0,
                "pid": 4242,
                "bounds": {
                    "x": 50,
                    "y": 60,
                    "width": 1024,
                    "height": 768
                },
                "bounds_changed": true,
                "source": "wmctrl"
            }
        }))
    );
    assert_eq!(
        fs::read_to_string(&action_path).unwrap(),
        "-ir 0x01200007 -e 0,50,60,1024,768"
    );

    let close = controller
        .close_window("0x01200007")
        .expect("real close backend");
    assert!(close.ok);
    assert_eq!(fs::read_to_string(&action_path).unwrap(), "-ic 0x01200007");
}

#[test]
fn real_window_input_uses_wmctrl_focus_and_xdotool() {
    let root = unique_temp_dir("bridgevm-tools-window-input");
    fs::create_dir_all(&root).unwrap();
    let wmctrl_path = root.join("wmctrl");
    let xdotool_path = root.join("xdotool");
    let action_path = root.join("actions.txt");
    fs::write(
        &wmctrl_path,
        format!(
            "#!/bin/sh\ncase \"$1\" in\n  -l) if [ \"$2\" = \"-p\" ] && [ \"$3\" = \"-G\" ]; then echo '0x01200007  0 4242 30 40 800 600 guest Terminal'; else echo '0x01200007  0 guest Terminal'; fi;;\n  -ia) printf 'wmctrl %s %s\\n' \"$1\" \"$2\" >> '{}';;\n  *) exit 2;;\nesac\n",
            action_path.display()
        ),
    )
    .unwrap();
    fs::write(
        &xdotool_path,
        format!(
            "#!/bin/sh\nprintf 'xdotool' >> '{}'\nfor arg in \"$@\"; do printf ' %s' \"$arg\" >> '{}'; done\nprintf '\\n' >> '{}'\n",
            action_path.display(),
            action_path.display(),
            action_path.display()
        ),
    )
    .unwrap();
    for path in [&wmctrl_path, &xdotool_path] {
        let mut permissions = fs::metadata(path).unwrap().permissions();
        permissions.set_mode(0o700);
        fs::set_permissions(path, permissions).unwrap();
    }

    let mut controller = DesktopController::real(None, Some(wmctrl_path), Some(xdotool_path));
    let pointer = controller
        .input_window(
            "0x01200007",
            &WindowInputEvent::Pointer {
                x: 120,
                y: 240,
                action: "click".to_string(),
                button: Some("left".to_string()),
            },
        )
        .expect("real input backend");

    assert!(pointer.ok);
    assert_eq!(
        pointer.result,
        Some(serde_json::json!({
            "window": {
                "id": "0x01200007",
                "title": "Terminal",
                "desktop": 0,
                "pid": 4242,
                "bounds": {
                    "x": 30,
                    "y": 40,
                    "width": 800,
                    "height": 600
                },
                "input": {
                    "kind": "pointer",
                        "x": 120,
                    "y": 240,
                    "action": "click",
                    "button": "left",
                    "source": "xdotool"
                },
                "source": "wmctrl"
            }
        }))
    );
    assert_eq!(
        fs::read_to_string(&action_path).unwrap(),
        "wmctrl -ia 0x01200007\nxdotool mousemove --sync 120 240\nxdotool click 1\n"
    );

    fs::write(&action_path, "").unwrap();
    let key = controller
        .input_window(
            "0x01200007",
            &WindowInputEvent::Key {
                key: "Return".to_string(),
                action: "tap".to_string(),
            },
        )
        .expect("real key input backend");
    assert!(key.ok);
    assert_eq!(
        fs::read_to_string(&action_path).unwrap(),
        "wmctrl -ia 0x01200007\nxdotool key Return\n"
    );
}

#[test]
fn explicit_clipboard_command_is_unchanged_by_detection() {
    // An explicit --clipboard-command path runs that exact program with no
    // extra args, regardless of capability auto-detection.
    let writer = resolve_clipboard_writer(
        &default_capabilities(),
        Some(PathBuf::from("/usr/local/bin/my-clipboard")),
    )
    .unwrap();
    let (program, args) = writer.command_for_test().expect("expected a command");
    assert_eq!(program, Path::new("/usr/local/bin/my-clipboard"));
    assert!(args.is_empty());

    // Likewise for the explicit display-resize command.
    let resizer = resolve_display_resizer(
        &default_capabilities(),
        Some(PathBuf::from("/usr/local/bin/my-resize")),
    )
    .unwrap();
    assert_eq!(
        resizer.command_for_test().expect("expected a command"),
        Path::new("/usr/local/bin/my-resize")
    );
}

#[test]
fn detection_only_runs_when_capability_enabled() {
    // No explicit command and no clipboard/display-resize capability ->
    // simulated, detection is never consulted.
    let heartbeat_only = [AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }];
    assert!(resolve_clipboard_writer(&heartbeat_only, None)
        .unwrap()
        .command_for_test()
        .is_none());
    assert!(resolve_display_resizer(&heartbeat_only, None)
        .unwrap()
        .command_for_test()
        .is_none());
}

#[test]
fn fire_and_forget_commands_do_not_emit_results() {
    let command = AgentEnvelope::new(AgentMessage::TimeSync {
        unix_epoch_millis: 1,
    });

    let mut state = GuestToolsState::new(&default_capabilities());
    assert_eq!(state.handle_command(&command), None);
}

#[test]
fn shared_folder_commands_update_guest_tools_state() {
    let mut state = GuestToolsState::new(&default_capabilities());
    let mount = AgentEnvelope::with_request_id(
        AgentMessage::MountShare {
            name: "workspace".to_string(),
            host_path_token: "host-token-1".to_string(),
        },
        "mount-1",
    );
    let update = AgentEnvelope::with_request_id(
        AgentMessage::MountShare {
            name: "workspace".to_string(),
            host_path_token: "host-token-2".to_string(),
        },
        "mount-2",
    );
    let unmount = AgentEnvelope::with_request_id(
        AgentMessage::UnmountShare {
            name: "workspace".to_string(),
        },
        "unmount-1",
    );

    assert_eq!(
        state.handle_command(&mount).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "mount-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted mount request for share workspace".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state
            .shared_folders
            .get("workspace")
            .map(|mount| mount.host_path_token.as_str()),
        Some("host-token-1")
    );
    assert_eq!(
        state.handle_command(&update).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "mount-2".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted share update for workspace".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state
            .shared_folders
            .get("workspace")
            .map(|mount| mount.host_path_token.as_str()),
        Some("host-token-2")
    );
    assert_eq!(
        state.handle_command(&unmount).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "unmount-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted unmount request for share workspace".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert!(!state.shared_folders.contains_key("workspace"));
}

#[test]
fn unmounting_missing_share_returns_error() {
    let mut state = GuestToolsState::new(&default_capabilities());
    let command = AgentEnvelope::with_request_id(
        AgentMessage::UnmountShare {
            name: "missing".to_string(),
        },
        "unmount-1",
    );

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "unmount-1".to_string(),
            ok: false,
            error_code: Some("share-not-mounted".to_string()),
            message: Some("share missing is not mounted".to_string()),
            result: None,
            metadata: None,
        }
    );
}

#[test]
fn shared_folder_commands_require_capability() {
    let mut state = GuestToolsState::new(&[AgentCapability {
        name: "heartbeat".to_string(),
        version: 1,
    }]);
    let command = AgentEnvelope::with_request_id(
        AgentMessage::MountShare {
            name: "workspace".to_string(),
            host_path_token: "host-token-1".to_string(),
        },
        "mount-1",
    );

    assert_eq!(
        state.handle_command(&command).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "mount-1".to_string(),
            ok: false,
            error_code: Some("capability-not-enabled".to_string()),
            message: Some("shared folders capability is not enabled".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert!(state.shared_folders.is_empty());
}

#[test]
fn file_drop_commands_track_alpha_transfer_state() {
    let mut state = GuestToolsState::new(&default_capabilities());
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

    assert_eq!(
        state.handle_command(&start).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-start-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("started file drop drop-1".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state.handle_command(&chunk).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-chunk-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("accepted file drop drop-1 chunk 0".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert_eq!(
        state.handle_command(&complete).unwrap().message,
        AgentMessage::CommandResult {
            request_id: "drop-complete-1".to_string(),
            ok: true,
            error_code: None,
            message: Some("completed file drop notes.txt (11 bytes across 1 chunks)".to_string()),
            result: None,
            metadata: None,
        }
    );
    assert!(state.file_drops.is_empty());
}
