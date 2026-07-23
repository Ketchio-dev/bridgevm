//! Split test module.

use crate::*;
use std::net::IpAddr;
use std::net::Ipv4Addr;
use std::net::Ipv6Addr;

use super::helpers::*;

#[test]
fn envelope_round_trips_guest_hello() {
    let envelope = AgentEnvelope::new(valid_guest_hello());

    let json = serde_json::to_string(&envelope).expect("serialize envelope");
    let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

    assert_eq!(decoded, envelope);
    assert_eq!(decoded.validate(), Ok(()));
}

#[test]
fn guest_hello_deserializes_legacy_shape_but_requires_new_handshake_fields() {
    let json = format!(
        r#"{{"protocol_version":{},"message":{{"GuestHello":{{"version":{},"guest_os":"linux"}}}}}}"#,
        PROTOCOL_VERSION, PROTOCOL_VERSION
    );
    let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

    assert_eq!(
        decoded.validate(),
        Err(ProtocolValidationError::EmptyCapabilities)
    );
}

#[test]
fn guest_ip_changed_round_trips_and_validates() {
    let envelope = AgentEnvelope::new(AgentMessage::GuestIpChanged {
        addresses: vec![
            GuestIpAddress {
                address: IpAddr::V4(Ipv4Addr::new(192, 168, 64, 2)),
                interface: Some("eth0".to_string()),
            },
            GuestIpAddress {
                address: IpAddr::V6(Ipv6Addr::LOCALHOST),
                interface: Some("lo".to_string()),
            },
        ],
    });

    let json = serde_json::to_string(&envelope).expect("serialize guest IP envelope");
    let decoded: AgentEnvelope =
        serde_json::from_str(&json).expect("deserialize guest IP envelope");

    assert_eq!(decoded, envelope);
    assert_eq!(decoded.validate(), Ok(()));
}

#[test]
fn agent_update_available_round_trips_and_validates() {
    let envelope = AgentEnvelope::new(AgentMessage::AgentUpdateAvailable {
        current_version: "1.2.3".to_string(),
        available_version: "1.2.4".to_string(),
        download_url: Some("https://updates.example.invalid/agent/1.2.4".to_string()),
        signature: Some("minisign-signature".to_string()),
    });

    let json = serde_json::to_string(&envelope).expect("serialize agent update envelope");
    let decoded: AgentEnvelope =
        serde_json::from_str(&json).expect("deserialize agent update envelope");

    assert_eq!(decoded, envelope);
    assert_eq!(decoded.validate(), Ok(()));
}

#[test]
fn p0_control_messages_round_trip_and_validate() {
    let messages = [
        AgentMessage::Heartbeat,
        AgentMessage::TimeSync {
            unix_epoch_millis: 1_781_470_000_000,
        },
        AgentMessage::SetClipboard {
            text: "hello from host".to_string(),
        },
        AgentMessage::ResizeDisplay {
            width: 1920,
            height: 1080,
            scale: 2,
        },
        AgentMessage::MountShare {
            name: "Projects".to_string(),
            host_path_token: "share-token-1".to_string(),
        },
        AgentMessage::UnmountShare {
            name: "Projects".to_string(),
        },
        AgentMessage::FileDropStart {
            transfer_id: "drop-1".to_string(),
            file_name: "notes.txt".to_string(),
            size_bytes: 11,
        },
        AgentMessage::FileDropChunk {
            transfer_id: "drop-1".to_string(),
            chunk_index: 0,
            data_base64: "aGVsbG8gd29ybGQ=".to_string(),
        },
        AgentMessage::FileDropComplete {
            transfer_id: "drop-1".to_string(),
        },
        AgentMessage::FreezeFilesystem {
            timeout_millis: Some(30_000),
        },
        AgentMessage::ThawFilesystem,
        AgentMessage::RunBenchmark {
            duration_millis: None,
        },
        AgentMessage::RunBenchmark {
            duration_millis: Some(500),
        },
        AgentMessage::WindowInput {
            id: "window-1".to_string(),
            event: WindowInputEvent::Pointer {
                x: 120,
                y: 240,
                action: "click".to_string(),
                button: Some("left".to_string()),
            },
        },
        AgentMessage::SetWindowBounds {
            id: "window-1".to_string(),
            x: 30,
            y: 40,
            width: 800,
            height: 600,
        },
        AgentMessage::WindowInput {
            id: "window-1".to_string(),
            event: WindowInputEvent::Key {
                key: "Return".to_string(),
                action: "tap".to_string(),
            },
        },
    ];

    for message in messages {
        let envelope = AgentEnvelope::new(message);
        let json = serde_json::to_string(&envelope).expect("serialize envelope");
        let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

        assert_eq!(decoded, envelope);
        assert_eq!(decoded.validate(), Ok(()));
    }
}

#[test]
fn envelope_request_id_round_trips_and_validates() {
    let envelope = AgentEnvelope::with_request_id(
        AgentMessage::ResizeDisplay {
            width: 1440,
            height: 900,
            scale: 2,
        },
        "resize-1",
    );

    let json = serde_json::to_string(&envelope).expect("serialize envelope");
    let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize envelope");

    assert_eq!(decoded, envelope);
    assert_eq!(decoded.validate(), Ok(()));
}

#[test]
fn command_result_round_trips_and_validates() {
    let envelope = AgentEnvelope::new(AgentMessage::CommandResult {
        request_id: "resize-1".to_string(),
        ok: false,
        error_code: Some("unsupported_resolution".to_string()),
        message: Some("guest refused 0x0 display".to_string()),
        result: Some(serde_json::json!({
            "width": 1440,
            "height": 900,
            "applied": false
        })),
        metadata: Some(serde_json::json!({
            "source": "display-agent",
            "attempt": 1
        })),
    });

    let json = serde_json::to_string(&envelope).expect("serialize command result");
    let decoded: AgentEnvelope = serde_json::from_str(&json).expect("deserialize command result");

    assert_eq!(decoded, envelope);
    assert_eq!(decoded.validate(), Ok(()));
}

#[test]
fn validation_rejects_wrong_envelope_version() {
    let envelope = AgentEnvelope {
        protocol_version: PROTOCOL_VERSION + 1,
        request_id: None,
        message: AgentMessage::Heartbeat,
    };

    assert_eq!(
        envelope.validate(),
        Err(ProtocolValidationError::UnsupportedVersion {
            expected: PROTOCOL_VERSION,
            actual: PROTOCOL_VERSION + 1,
        })
    );
}

#[test]
fn validation_rejects_empty_guest_ip_report() {
    let envelope = AgentEnvelope::new(AgentMessage::GuestIpChanged { addresses: vec![] });

    assert_eq!(
        envelope.validate(),
        Err(ProtocolValidationError::EmptyGuestIpList)
    );
}

#[test]
fn validation_rejects_unspecified_guest_ip_report() {
    let envelope = AgentEnvelope::new(AgentMessage::GuestIpChanged {
        addresses: vec![GuestIpAddress {
            address: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            interface: Some("eth0".to_string()),
        }],
    });

    assert_eq!(
        envelope.validate(),
        Err(ProtocolValidationError::UnspecifiedGuestIp {
            address: IpAddr::V4(Ipv4Addr::UNSPECIFIED)
        })
    );
}

#[test]
fn validation_rejects_invalid_p0_control_messages() {
    let cases = [
        (
            AgentMessage::TimeSync {
                unix_epoch_millis: 0,
            },
            ProtocolValidationError::InvalidTimestamp,
        ),
        (
            AgentMessage::ResizeDisplay {
                width: 0,
                height: 1080,
                scale: 2,
            },
            ProtocolValidationError::InvalidDisplaySize {
                width: 0,
                height: 1080,
                scale: 2,
            },
        ),
        (
            AgentMessage::MountShare {
                name: " ".to_string(),
                host_path_token: "share-token-1".to_string(),
            },
            ProtocolValidationError::EmptyField {
                field: "share.name",
            },
        ),
        (
            AgentMessage::MountShare {
                name: "Projects".to_string(),
                host_path_token: "".to_string(),
            },
            ProtocolValidationError::EmptyField {
                field: "share.host_path_token",
            },
        ),
        (
            AgentMessage::LaunchApplication {
                id: "\t".to_string(),
            },
            ProtocolValidationError::EmptyField {
                field: "application.id",
            },
        ),
        (
            AgentMessage::SetWindowBounds {
                id: " ".to_string(),
                x: 0,
                y: 0,
                width: 800,
                height: 600,
            },
            ProtocolValidationError::EmptyField { field: "window.id" },
        ),
        (
            AgentMessage::SetWindowBounds {
                id: "window-1".to_string(),
                x: 0,
                y: 0,
                width: 0,
                height: 600,
            },
            ProtocolValidationError::InvalidWindowBounds {
                width: 0,
                height: 600,
            },
        ),
        (
            AgentMessage::WindowInput {
                id: " ".to_string(),
                event: WindowInputEvent::Pointer {
                    x: 1,
                    y: 2,
                    action: "move".to_string(),
                    button: None,
                },
            },
            ProtocolValidationError::EmptyField { field: "window.id" },
        ),
        (
            AgentMessage::WindowInput {
                id: "window-1".to_string(),
                event: WindowInputEvent::Pointer {
                    x: 1,
                    y: 2,
                    action: "drag".to_string(),
                    button: Some("left".to_string()),
                },
            },
            ProtocolValidationError::InvalidWindowInputValue {
                field: "window_input.pointer.action",
                value: "drag".to_string(),
            },
        ),
        (
            AgentMessage::WindowInput {
                id: "window-1".to_string(),
                event: WindowInputEvent::Pointer {
                    x: 1,
                    y: 2,
                    action: "click".to_string(),
                    button: None,
                },
            },
            ProtocolValidationError::MissingField {
                field: "window_input.pointer.button",
            },
        ),
        (
            AgentMessage::WindowInput {
                id: "window-1".to_string(),
                event: WindowInputEvent::Pointer {
                    x: 1,
                    y: 2,
                    action: "click".to_string(),
                    button: Some("button-9".to_string()),
                },
            },
            ProtocolValidationError::InvalidWindowInputValue {
                field: "window_input.pointer.button",
                value: "button-9".to_string(),
            },
        ),
        (
            AgentMessage::WindowInput {
                id: "window-1".to_string(),
                event: WindowInputEvent::Key {
                    key: "".to_string(),
                    action: "tap".to_string(),
                },
            },
            ProtocolValidationError::EmptyField {
                field: "window_input.key",
            },
        ),
        (
            AgentMessage::WindowInput {
                id: "window-1".to_string(),
                event: WindowInputEvent::Key {
                    key: "Return".to_string(),
                    action: "repeat".to_string(),
                },
            },
            ProtocolValidationError::InvalidWindowInputValue {
                field: "window_input.key.action",
                value: "repeat".to_string(),
            },
        ),
        (
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 0,
            },
            ProtocolValidationError::InvalidFileDropSize,
        ),
        (
            AgentMessage::FileDropChunk {
                transfer_id: " ".to_string(),
                chunk_index: 0,
                data_base64: "aGVsbG8=".to_string(),
            },
            ProtocolValidationError::EmptyField {
                field: "file_drop.transfer_id",
            },
        ),
        (
            AgentMessage::GuestMetrics {
                cpu_percent: 101,
                memory_used_mib: 1024,
            },
            ProtocolValidationError::InvalidCpuPercent(101),
        ),
        (
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(0),
            },
            ProtocolValidationError::InvalidFilesystemFreezeTimeout {
                timeout_millis: 0,
                max_timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS,
            },
        ),
        (
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(MAX_FREEZE_THAW_TIMEOUT_MILLIS + 1),
            },
            ProtocolValidationError::InvalidFilesystemFreezeTimeout {
                timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS + 1,
                max_timeout_millis: MAX_FREEZE_THAW_TIMEOUT_MILLIS,
            },
        ),
        (
            AgentMessage::RunBenchmark {
                duration_millis: Some(0),
            },
            ProtocolValidationError::InvalidBenchmarkDuration {
                duration_millis: 0,
                max_duration_millis: MAX_BENCHMARK_DURATION_MILLIS,
            },
        ),
        (
            AgentMessage::RunBenchmark {
                duration_millis: Some(MAX_BENCHMARK_DURATION_MILLIS + 1),
            },
            ProtocolValidationError::InvalidBenchmarkDuration {
                duration_millis: MAX_BENCHMARK_DURATION_MILLIS + 1,
                max_duration_millis: MAX_BENCHMARK_DURATION_MILLIS,
            },
        ),
        (
            AgentMessage::CommandResult {
                request_id: "".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            },
            ProtocolValidationError::EmptyField {
                field: "command_result.request_id",
            },
        ),
        (
            AgentMessage::CommandResult {
                request_id: "resize-1".to_string(),
                ok: false,
                error_code: Some(" ".to_string()),
                message: None,
                result: None,
                metadata: None,
            },
            ProtocolValidationError::EmptyField {
                field: "command_result.error_code",
            },
        ),
        (
            AgentMessage::CommandResult {
                request_id: "resize-1".to_string(),
                ok: false,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            },
            ProtocolValidationError::MissingField {
                field: "command_result.error_code",
            },
        ),
        (
            AgentMessage::AgentUpdateAvailable {
                current_version: "".to_string(),
                available_version: "1.2.4".to_string(),
                download_url: None,
                signature: None,
            },
            ProtocolValidationError::EmptyField {
                field: "agent_update.current_version",
            },
        ),
        (
            AgentMessage::AgentUpdateAvailable {
                current_version: "1.2.3".to_string(),
                available_version: " ".to_string(),
                download_url: None,
                signature: None,
            },
            ProtocolValidationError::EmptyField {
                field: "agent_update.available_version",
            },
        ),
        (
            AgentMessage::AgentUpdateAvailable {
                current_version: "1.2.3".to_string(),
                available_version: "1.2.4".to_string(),
                download_url: Some("\t".to_string()),
                signature: None,
            },
            ProtocolValidationError::EmptyField {
                field: "agent_update.download_url",
            },
        ),
        (
            AgentMessage::AgentUpdateAvailable {
                current_version: "1.2.3".to_string(),
                available_version: "1.2.4".to_string(),
                download_url: None,
                signature: Some(" ".to_string()),
            },
            ProtocolValidationError::EmptyField {
                field: "agent_update.signature",
            },
        ),
    ];

    for (message, expected) in cases {
        let envelope = AgentEnvelope::new(message);
        assert_eq!(envelope.validate(), Err(expected));
    }
}

#[test]
fn validation_rejects_empty_envelope_request_id() {
    let envelope = AgentEnvelope::with_request_id(AgentMessage::Heartbeat, " ");

    assert_eq!(
        envelope.validate(),
        Err(ProtocolValidationError::EmptyField {
            field: "request_id"
        })
    );
}

#[test]
fn validation_rejects_invalid_guest_hello_handshake() {
    let cases = [
        (
            AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![],
                auth: Some(AgentAuth::ToolsToken {
                    token: "vm-token".to_string(),
                }),
            },
            ProtocolValidationError::EmptyCapabilities,
        ),
        (
            AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some(" ".to_string()),
                capabilities: valid_capabilities(),
                auth: Some(AgentAuth::ToolsToken {
                    token: "vm-token".to_string(),
                }),
            },
            ProtocolValidationError::EmptyField {
                field: "guest_hello.agent_version",
            },
        ),
        (
            AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: vec![AgentCapability {
                    name: "clipboard".to_string(),
                    version: 0,
                }],
                auth: Some(AgentAuth::ToolsToken {
                    token: "vm-token".to_string(),
                }),
            },
            ProtocolValidationError::InvalidCapabilityVersion {
                capability: "clipboard".to_string(),
                version: 0,
            },
        ),
        (
            AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: valid_capabilities(),
                auth: None,
            },
            ProtocolValidationError::MissingField {
                field: "guest_hello.auth",
            },
        ),
        (
            AgentMessage::GuestHello {
                version: PROTOCOL_VERSION,
                guest_os: "linux".to_string(),
                agent_version: Some("1.0.0".to_string()),
                capabilities: valid_capabilities(),
                auth: Some(AgentAuth::ToolsToken {
                    token: "".to_string(),
                }),
            },
            ProtocolValidationError::EmptyField {
                field: "auth.tools_token",
            },
        ),
    ];

    for (message, expected) in cases {
        let envelope = AgentEnvelope::new(message);
        assert_eq!(envelope.validate(), Err(expected));
    }
}
