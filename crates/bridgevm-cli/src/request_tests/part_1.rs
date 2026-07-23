//! Tests split so no file exceeds 1000 lines.

use crate::*;

#[test]
fn guest_tools_set_clipboard_cli_builds_host_command_envelope() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "set-clipboard",
        "dev",
        "--text",
        "hello from host",
        "--request-id",
        "clipboard-1",
    ])
    .unwrap();

    let request = request_for(cli.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("clipboard-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::SetClipboard {
            text: "hello from host".to_string(),
        }
    );
}

#[test]
fn guest_tools_resize_display_cli_builds_host_command_envelope() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "resize-display",
        "dev",
        "--width",
        "1440",
        "--height",
        "900",
        "--scale",
        "2",
        "--request-id",
        "resize-1",
    ])
    .unwrap();

    let request = request_for(cli.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("resize-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::ResizeDisplay {
            width: 1440,
            height: 900,
            scale: 2,
        }
    );
}

#[test]
fn qmp_control_cli_builds_typed_requests() {
    let cli = Cli::try_parse_from(["bridgevm", "qmp-stop", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    let BridgeVmRequest::QmpStop { name } = request else {
        panic!("expected qmp stop request");
    };
    assert_eq!(name, "dev");

    let cli = Cli::try_parse_from(["bridgevm", "qmp-cont", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    let BridgeVmRequest::QmpCont { name } = request else {
        panic!("expected qmp cont request");
    };
    assert_eq!(name, "dev");
}

#[test]
fn lifecycle_plan_cli_builds_typed_request() {
    let cli =
        Cli::try_parse_from(["bridgevm", "lifecycle-plan", "dev", "--action", "resume"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::LifecyclePlan {
            name: "dev".to_string(),
            action: LifecycleAction::Resume,
        }
    );
}

#[test]
fn resources_reapply_cli_builds_typed_request() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "resources",
        "reapply",
        "dev",
        "--visibility",
        "background",
    ])
    .unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::ReapplyRuntimeResources {
            name: "dev".to_string(),
            visibility: RuntimeResourceVisibility::Background,
        }
    );
}

#[test]
fn runtime_control_cli_builds_typed_requests() {
    let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "status", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::RuntimeControl {
            name: "dev".to_string(),
            command: "status".to_string(),
        }
    );

    let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "stop", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::RuntimeControl {
            name: "dev".to_string(),
            command: "stop".to_string(),
        }
    );

    let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "policy", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::RuntimeControl {
            name: "dev".to_string(),
            command: "policy".to_string(),
        }
    );

    let cli = Cli::try_parse_from(["bridgevm", "runtime-control", "pacing", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::RuntimeControl {
            name: "dev".to_string(),
            command: "pacing".to_string(),
        }
    );

    let cli = Cli::try_parse_from([
        "bridgevm",
        "runtime-control",
        "reapply",
        "dev",
        "--visibility",
        "background",
    ])
    .unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::ReapplyRuntimeResources {
            name: "dev".to_string(),
            visibility: RuntimeResourceVisibility::Background,
        }
    );
}

#[test]
fn readiness_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "readiness", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::ReadinessReport {
            name: "dev".to_string(),
            live_evidence: None,
            record_live_evidence: false,
            clear_live_evidence: false,
        }
    );

    let cli = Cli::try_parse_from([
        "bridgevm",
        "readiness",
        "dev",
        "--live-evidence",
        "/tmp/live",
        "--record-live-evidence",
    ])
    .unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::ReadinessReport {
            name: "dev".to_string(),
            live_evidence: Some(PathBuf::from("/tmp/live")),
            record_live_evidence: true,
            clear_live_evidence: false,
        }
    );

    let cli =
        Cli::try_parse_from(["bridgevm", "readiness", "dev", "--clear-live-evidence"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::ReadinessReport {
            name: "dev".to_string(),
            live_evidence: None,
            record_live_evidence: false,
            clear_live_evidence: true,
        }
    );
}

#[test]
fn guest_tools_filesystem_cli_builds_host_command_envelopes() {
    let freeze = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "freeze-filesystem",
        "dev",
        "--request-id",
        "freeze-1",
        "--timeout-millis",
        "5000",
    ])
    .unwrap();
    let request = request_for(freeze.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("freeze-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::FreezeFilesystem {
            timeout_millis: Some(5_000),
        }
    );

    let thaw = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "thaw-filesystem",
        "dev",
        "--request-id",
        "thaw-1",
    ])
    .unwrap();
    let request = request_for(thaw.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("thaw-1"));
    assert_eq!(envelope.message, AgentMessage::ThawFilesystem);
}

#[test]
fn guest_tools_file_drop_cli_builds_host_command_envelopes() {
    let start = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "file-drop-start",
        "dev",
        "--transfer-id",
        "drop-1",
        "--file-name",
        "notes.txt",
        "--size-bytes",
        "12",
        "--request-id",
        "drop-start-1",
    ])
    .unwrap();
    let request = request_for(start.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("drop-start-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::FileDropStart {
            transfer_id: "drop-1".to_string(),
            file_name: "notes.txt".to_string(),
            size_bytes: 12,
        }
    );

    let chunk = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "file-drop-chunk",
        "dev",
        "--transfer-id",
        "drop-1",
        "--chunk-index",
        "0",
        "--data-base64",
        "aGVsbG8=",
        "--request-id",
        "drop-chunk-1",
    ])
    .unwrap();
    let request = request_for(chunk.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("drop-chunk-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::FileDropChunk {
            transfer_id: "drop-1".to_string(),
            chunk_index: 0,
            data_base64: "aGVsbG8=".to_string(),
        }
    );

    let complete = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "file-drop-complete",
        "dev",
        "--transfer-id",
        "drop-1",
        "--request-id",
        "drop-complete-1",
    ])
    .unwrap();
    let request = request_for(complete.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("drop-complete-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::FileDropComplete {
            transfer_id: "drop-1".to_string(),
        }
    );
}

#[test]
fn guest_tools_application_cli_builds_host_command_envelopes() {
    let list = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "list-applications",
        "dev",
        "--request-id",
        "apps-1",
    ])
    .unwrap();
    let request = request_for(list.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("apps-1"));
    assert_eq!(envelope.message, AgentMessage::ListApplications);

    let launch = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "launch-application",
        "dev",
        "--id",
        "terminal",
        "--request-id",
        "launch-1",
    ])
    .unwrap();
    let request = request_for(launch.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("launch-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::LaunchApplication {
            id: "terminal".to_string(),
        }
    );
}

#[test]
fn guest_tools_window_cli_builds_host_command_envelopes() {
    let list = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "list-windows",
        "dev",
        "--request-id",
        "windows-1",
    ])
    .unwrap();
    let request = request_for(list.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("windows-1"));
    assert_eq!(envelope.message, AgentMessage::ListWindows);

    let focus = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "focus-window",
        "dev",
        "--id",
        "window-terminal",
        "--request-id",
        "focus-1",
    ])
    .unwrap();
    let request = request_for(focus.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("focus-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::FocusWindow {
            id: "window-terminal".to_string(),
        }
    );

    let close = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "close-window",
        "dev",
        "--id",
        "window-terminal",
        "--request-id",
        "close-1",
    ])
    .unwrap();
    let request = request_for(close.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("close-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::CloseWindow {
            id: "window-terminal".to_string(),
        }
    );

    let bounds = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "set-window-bounds",
        "dev",
        "--id",
        "window-terminal",
        "--x",
        "30",
        "--y",
        "40",
        "--width",
        "800",
        "--height",
        "600",
        "--request-id",
        "bounds-1",
    ])
    .unwrap();
    let request = request_for(bounds.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("bounds-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::SetWindowBounds {
            id: "window-terminal".to_string(),
            x: 30,
            y: 40,
            width: 800,
            height: 600,
        }
    );

    let pointer = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "window-pointer",
        "dev",
        "--id",
        "window-terminal",
        "--x",
        "120",
        "--y",
        "240",
        "--action",
        "click",
        "--button",
        "left",
        "--request-id",
        "pointer-1",
    ])
    .unwrap();
    let request = request_for(pointer.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("pointer-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::WindowInput {
            id: "window-terminal".to_string(),
            event: WindowInputEvent::Pointer {
                x: 120,
                y: 240,
                action: "click".to_string(),
                button: Some("left".to_string()),
            },
        }
    );

    let key = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "window-key",
        "dev",
        "--id",
        "window-terminal",
        "--key",
        "Return",
        "--action",
        "tap",
        "--request-id",
        "key-1",
    ])
    .unwrap();
    let request = request_for(key.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("key-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::WindowInput {
            id: "window-terminal".to_string(),
            event: WindowInputEvent::Key {
                key: "Return".to_string(),
                action: "tap".to_string(),
            },
        }
    );
}

#[test]
fn guest_tools_time_sync_cli_builds_host_command_envelope() {
    let cli = Cli::try_parse_from([
        "bridgevm",
        "guest-tools",
        "time-sync",
        "dev",
        "--unix-epoch-millis",
        "1",
        "--request-id",
        "time-sync-1",
    ])
    .unwrap();

    let request = request_for(cli.command).unwrap();
    let BridgeVmRequest::GuestToolsSendCommand { name, envelope } = request else {
        panic!("expected guest tools send command request");
    };

    assert_eq!(name, "dev");
    assert_eq!(envelope.request_id.as_deref(), Some("time-sync-1"));
    assert_eq!(
        envelope.message,
        AgentMessage::TimeSync {
            unix_epoch_millis: 1,
        }
    );
}

#[test]
fn network_plan_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "network-plan", "legacy"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::PlanNetwork {
            name: "legacy".to_string(),
        }
    );
}

#[test]
fn ssh_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "ssh", "dev", "--user", "ubuntu"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::SshPlan {
            name: "dev".to_string(),
            user: Some("ubuntu".to_string()),
        }
    );
}

#[test]
fn restart_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "restart", "dev"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::RestartVm {
            name: "dev".to_string(),
        }
    );
}

#[test]
fn open_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "open", "dev", "80", "--scheme", "https"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::OpenPort {
            name: "dev".to_string(),
            guest: 80,
            scheme: Some("https".to_string()),
        }
    );
}

#[test]
fn clone_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "clone", "dev", "dev-copy"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::CloneVm {
            name: "dev".to_string(),
            new_name: "dev-copy".to_string(),
            linked: false,
        }
    );
}

#[test]
fn linked_clone_cli_builds_typed_request() {
    let cli = Cli::try_parse_from(["bridgevm", "clone", "dev", "dev-copy", "--linked"]).unwrap();
    let request = request_for(cli.command).unwrap();
    assert_eq!(
        request,
        BridgeVmRequest::CloneVm {
            name: "dev".to_string(),
            new_name: "dev-copy".to_string(),
            linked: true,
        }
    );
}
