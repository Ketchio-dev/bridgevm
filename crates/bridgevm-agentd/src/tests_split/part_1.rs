//! Split test module.

use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::ProtocolValidationError;
use bridgevm_agent_protocol::PROTOCOL_VERSION;
use std::io::Cursor;

use super::helpers::*;

#[test]
fn accepts_authenticated_guest_hello_with_allowed_capabilities() {
    let envelope = AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities()));
    let session = accept_guest_hello(&envelope, &valid_policy()).unwrap();

    assert_eq!(session.guest_os, "linux");
    assert_eq!(session.agent_version.as_deref(), Some("1.0.0"));
    assert!(session.supports("clipboard"));
    assert!(!session.supports("heartbeat"));
    assert!(!session.supports("drag-drop"));
    assert_eq!(session.capability_version("clipboard"), Some(1));
    assert_eq!(session.capability_version("drag-drop"), None);
}

#[test]
fn rejects_non_hello_messages() {
    let envelope = AgentEnvelope::new(AgentMessage::Heartbeat);

    assert_eq!(
        accept_guest_hello(&envelope, &valid_policy()),
        Err(AgentdError::ExpectedGuestHello)
    );
}

#[test]
fn rejects_invalid_protocol_handshake() {
    let envelope = AgentEnvelope::new(AgentMessage::GuestHello {
        version: PROTOCOL_VERSION,
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: vec![],
        auth: Some(AgentAuth::ToolsToken {
            token: "token-1".to_string(),
        }),
    });

    assert_eq!(
        accept_guest_hello(&envelope, &valid_policy()),
        Err(AgentdError::Protocol(
            ProtocolValidationError::EmptyCapabilities
        ))
    );
}

#[test]
fn rejects_wrong_tools_token() {
    let envelope = AgentEnvelope::new(valid_guest_hello("wrong-token", valid_capabilities()));

    assert_eq!(
        accept_guest_hello(&envelope, &valid_policy()),
        Err(AgentdError::InvalidToolsToken)
    );
}

#[test]
fn rejects_duplicate_capabilities() {
    let envelope = AgentEnvelope::new(valid_guest_hello(
        "token-1",
        vec![
            AgentCapability {
                name: "clipboard".to_string(),
                version: 1,
            },
            AgentCapability {
                name: "clipboard".to_string(),
                version: 1,
            },
        ],
    ));

    assert_eq!(
        accept_guest_hello(&envelope, &valid_policy()),
        Err(AgentdError::DuplicateCapability {
            capability: "clipboard".to_string()
        })
    );
}

#[test]
fn rejects_capabilities_outside_vm_policy() {
    let envelope = AgentEnvelope::new(valid_guest_hello(
        "token-1",
        vec![AgentCapability {
            name: "drag-drop".to_string(),
            version: 1,
        }],
    ));

    assert_eq!(
        accept_guest_hello(&envelope, &valid_policy()),
        Err(AgentdError::CapabilityNotAllowed {
            capability: "drag-drop".to_string()
        })
    );
}

#[test]
fn rejects_capability_versions_newer_than_host_policy() {
    let envelope = AgentEnvelope::new(valid_guest_hello(
        "token-1",
        vec![AgentCapability {
            name: "clipboard".to_string(),
            version: 2,
        }],
    ));

    assert_eq!(
        accept_guest_hello(&envelope, &valid_policy()),
        Err(AgentdError::CapabilityVersionTooNew {
            capability: "clipboard".to_string(),
            max_version: 1,
            actual_version: 2,
        })
    );
}

#[test]
fn rejects_invalid_policy_before_accepting_guest() {
    let envelope = AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities()));
    let policy = AgentPolicy::new("", [("heartbeat", 1)]);

    assert_eq!(
        accept_guest_hello(&envelope, &policy),
        Err(AgentdError::EmptyPolicyField {
            field: "expected_tools_token"
        })
    );
}

#[test]
fn rejects_invalid_allowed_capability_version() {
    let envelope = AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities()));
    let policy = AgentPolicy::new("token-1", [("heartbeat", 0)]);

    assert_eq!(
        accept_guest_hello(&envelope, &policy),
        Err(AgentdError::InvalidAllowedCapabilityVersion)
    );
}

#[test]
fn maps_messages_to_required_capabilities() {
    let cases = [
        (AgentMessage::Heartbeat, None),
        (
            AgentMessage::CommandResult {
                request_id: "resize-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            },
            None,
        ),
        (
            AgentMessage::AgentUpdateAvailable {
                current_version: "0.1.0".to_string(),
                available_version: "0.1.1".to_string(),
                download_url: None,
                signature: None,
            },
            Some("agent-update"),
        ),
        (
            AgentMessage::TimeSync {
                unix_epoch_millis: 1,
            },
            Some("time-sync"),
        ),
        (
            AgentMessage::GuestIpChanged {
                addresses: vec![bridgevm_agent_protocol::GuestIpAddress {
                    address: "192.168.64.2".parse().unwrap(),
                    interface: Some("eth0".to_string()),
                }],
            },
            Some("guest-ip"),
        ),
        (
            AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
            Some("clipboard"),
        ),
        (
            AgentMessage::ResizeDisplay {
                width: 1920,
                height: 1080,
                scale: 2,
            },
            Some("display-resize"),
        ),
        (
            AgentMessage::MountShare {
                name: "Projects".to_string(),
                host_path_token: "share-token".to_string(),
            },
            Some("shared-folders"),
        ),
        (
            AgentMessage::FileDropStart {
                transfer_id: "drop-1".to_string(),
                file_name: "notes.txt".to_string(),
                size_bytes: 12,
            },
            Some("drag-drop"),
        ),
        (AgentMessage::ListApplications, Some("applications")),
        (AgentMessage::ListWindows, Some("windows")),
        (
            AgentMessage::SetWindowBounds {
                id: "window-1".to_string(),
                x: 30,
                y: 40,
                width: 800,
                height: 600,
            },
            Some("windows"),
        ),
        (
            AgentMessage::WindowInput {
                id: "window-1".to_string(),
                event: bridgevm_agent_protocol::WindowInputEvent::Pointer {
                    x: 10,
                    y: 20,
                    action: "click".to_string(),
                    button: Some("left".to_string()),
                },
            },
            Some("windows"),
        ),
        (
            AgentMessage::GuestMetrics {
                cpu_percent: 5,
                memory_used_mib: 1024,
            },
            Some("guest-metrics"),
        ),
        (
            AgentMessage::FreezeFilesystem {
                timeout_millis: Some(5_000),
            },
            Some("fs-freeze"),
        ),
        (AgentMessage::ThawFilesystem, Some("fs-thaw")),
        (
            AgentMessage::RunBenchmark {
                duration_millis: Some(500),
            },
            Some("benchmark"),
        ),
    ];

    for (message, expected) in cases {
        assert_eq!(required_capability(&message), expected);
    }
}

#[test]
fn authorizes_messages_supported_by_session_capabilities() {
    let session = accept_guest_hello(
        &AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities())),
        &valid_policy(),
    )
    .unwrap();

    assert_eq!(
        authorize_message(&session, &AgentMessage::Heartbeat),
        Ok(())
    );
    assert_eq!(
        authorize_message(
            &session,
            &AgentMessage::SetClipboard {
                text: "hello".to_string()
            }
        ),
        Ok(())
    );
}

#[test]
fn rejects_messages_not_supported_by_session_capabilities() {
    let session = accept_guest_hello(
        &AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities())),
        &valid_policy(),
    )
    .unwrap();

    assert_eq!(
        authorize_message(
            &session,
            &AgentMessage::ResizeDisplay {
                width: 1920,
                height: 1080,
                scale: 2,
            }
        ),
        Err(AgentdError::CommandNotAuthorized {
            capability: "display-resize".to_string()
        })
    );
}

#[test]
fn rejects_benchmark_command_without_benchmark_capability() {
    // Session advertises only `clipboard`, so a benchmark request must be
    // rejected as not authorized (mirrors display-resize gating above).
    let session = accept_guest_hello(
        &AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities())),
        &valid_policy(),
    )
    .unwrap();

    assert_eq!(
        authorize_message(
            &session,
            &AgentMessage::RunBenchmark {
                duration_millis: Some(500),
            }
        ),
        Err(AgentdError::CommandNotAuthorized {
            capability: "benchmark".to_string()
        })
    );
}

#[test]
fn authorizes_benchmark_command_with_benchmark_capability() {
    let policy = AgentPolicy::new("token-1", [("benchmark", 1)]);
    let session = accept_guest_hello(
        &AgentEnvelope::new(valid_guest_hello(
            "token-1",
            vec![AgentCapability {
                name: "benchmark".to_string(),
                version: 1,
            }],
        )),
        &policy,
    )
    .unwrap();

    assert_eq!(
        authorize_message(
            &session,
            &AgentMessage::RunBenchmark {
                duration_millis: Some(500),
            }
        ),
        Ok(())
    );
}

#[test]
fn tracks_pending_host_commands_until_matching_command_result() {
    let session = valid_session();
    let mut tracker = AgentCommandTracker::new();
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello".to_string(),
        },
        "clipboard-1",
    );

    tracker.begin_host_command(&session, &command).unwrap();
    assert_eq!(tracker.pending_count(), 1);
    assert!(tracker.has_pending("clipboard-1"));

    let completed = tracker
        .complete_command_result(&AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: "clipboard-1".to_string(),
            ok: true,
            error_code: None,
            message: None,
            result: None,
            metadata: None,
        }))
        .unwrap();

    assert_eq!(
        completed,
        PendingCommand {
            request_id: "clipboard-1".to_string(),
            capability: Some("clipboard".to_string()),
            message: AgentMessage::SetClipboard {
                text: "hello".to_string(),
            },
        }
    );
    assert_eq!(tracker.pending_count(), 0);
}

#[test]
fn does_not_track_fire_and_forget_host_commands_without_request_id() {
    let session = valid_session();
    let mut tracker = AgentCommandTracker::new();

    tracker
        .begin_host_command(
            &session,
            &AgentEnvelope::new(AgentMessage::SetClipboard {
                text: "hello".to_string(),
            }),
        )
        .unwrap();

    assert_eq!(tracker.pending_count(), 0);
}

#[test]
fn rejects_duplicate_pending_request_ids() {
    let session = valid_session();
    let mut tracker = AgentCommandTracker::new();
    let command = AgentEnvelope::with_request_id(
        AgentMessage::SetClipboard {
            text: "hello".to_string(),
        },
        "clipboard-1",
    );

    tracker.begin_host_command(&session, &command).unwrap();

    assert_eq!(
        tracker.begin_host_command(&session, &command),
        Err(AgentdError::PendingRequestExists {
            request_id: "clipboard-1".to_string(),
        })
    );
}

#[test]
fn rejects_unexpected_command_results() {
    let mut tracker = AgentCommandTracker::new();

    assert_eq!(
        tracker.complete_command_result(&AgentEnvelope::new(AgentMessage::CommandResult {
            request_id: "missing".to_string(),
            ok: true,
            error_code: None,
            message: None,
            result: None,
            metadata: None,
        })),
        Err(AgentdError::UnexpectedCommandResult {
            request_id: "missing".to_string(),
        })
    );
}

#[test]
fn rejects_wrong_message_direction_for_tracker_operations() {
    let session = valid_session();
    let mut tracker = AgentCommandTracker::new();

    assert_eq!(
        tracker.begin_host_command(
            &session,
            &AgentEnvelope::new(AgentMessage::CommandResult {
                request_id: "clipboard-1".to_string(),
                ok: true,
                error_code: None,
                message: None,
                result: None,
                metadata: None,
            }),
        ),
        Err(AgentdError::ExpectedHostCommand)
    );
    assert_eq!(
        tracker.complete_command_result(&AgentEnvelope::new(AgentMessage::Heartbeat)),
        Err(AgentdError::ExpectedCommandResult)
    );
}

#[test]
fn encodes_and_decodes_valid_envelope_lines() {
    let envelope = AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities()));

    let line = encode_envelope_line(&envelope).unwrap();
    assert!(line.ends_with('\n'));
    assert_eq!(decode_envelope_line(&line), Ok(envelope));
}

#[test]
fn codec_rejects_invalid_frames() {
    assert_eq!(decode_envelope_line(""), Err(AgentCodecError::EmptyFrame));
    assert_eq!(
        decode_envelope_line("{}"),
        Err(AgentCodecError::MissingFrameTerminator)
    );
    assert_eq!(
        decode_envelope_line("{}\n{}\n"),
        Err(AgentCodecError::MultipleFrames)
    );
    assert!(matches!(
        decode_envelope_line("not-json\n"),
        Err(AgentCodecError::Json(_))
    ));
}

#[test]
fn codec_rejects_invalid_envelopes() {
    let envelope = AgentEnvelope::new(AgentMessage::GuestHello {
        version: PROTOCOL_VERSION,
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: vec![],
        auth: Some(AgentAuth::ToolsToken {
            token: "token-1".to_string(),
        }),
    });

    assert_eq!(
        encode_envelope_line(&envelope),
        Err(AgentCodecError::Protocol(
            ProtocolValidationError::EmptyCapabilities
        ))
    );

    let line = format!(
        "{}\n",
        serde_json::to_string(&envelope).expect("serialize invalid envelope")
    );
    assert_eq!(
        decode_envelope_line(&line),
        Err(AgentCodecError::Protocol(
            ProtocolValidationError::EmptyCapabilities
        ))
    );
}

#[test]
fn io_helpers_round_trip_valid_envelope_lines() {
    let envelope = AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities()));
    let mut buffer = Cursor::new(Vec::new());

    write_envelope_line(&mut buffer, &envelope).unwrap();
    buffer.set_position(0);

    assert_eq!(read_envelope_line(&mut buffer), Ok(Some(envelope)));
    assert_eq!(read_envelope_line(&mut buffer), Ok(None));
}

#[test]
fn read_envelope_line_returns_none_on_clean_eof() {
    let mut buffer = Cursor::new(Vec::new());

    assert_eq!(read_envelope_line(&mut buffer), Ok(None));
}

#[test]
fn read_envelope_line_rejects_partial_frames() {
    let mut buffer = Cursor::new(br#"{"protocol_version":1}"#.to_vec());

    assert_eq!(
        read_envelope_line(&mut buffer),
        Err(AgentCodecError::MissingFrameTerminator)
    );
}

#[test]
fn read_envelope_line_rejects_oversized_frame() {
    // A hostile guest streaming bytes with no newline must not be able to
    // grow the host's read buffer without bound.
    let mut huge = vec![b'A'; MAX_FRAME_BYTES + 16];
    // (no trailing newline on purpose)
    let mut buffer = Cursor::new(std::mem::take(&mut huge));
    assert_eq!(
        read_envelope_line(&mut buffer),
        Err(AgentCodecError::FrameTooLarge)
    );
}

#[test]
fn constant_time_eq_matches_only_equal_bytes() {
    assert!(constant_time_eq(b"abc123", b"abc123"));
    assert!(!constant_time_eq(b"abc123", b"abc124"));
    assert!(!constant_time_eq(b"abc", b"abcd"));
    assert!(constant_time_eq(b"", b""));
}

#[test]
fn read_envelope_line_rejects_invalid_json_and_invalid_envelopes() {
    let mut invalid_json = Cursor::new(b"not-json\n".to_vec());
    assert!(matches!(
        read_envelope_line(&mut invalid_json),
        Err(AgentCodecError::Json(_))
    ));

    let envelope = AgentEnvelope::new(AgentMessage::GuestHello {
        version: PROTOCOL_VERSION,
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: vec![],
        auth: Some(AgentAuth::ToolsToken {
            token: "token-1".to_string(),
        }),
    });
    let line = format!(
        "{}\n",
        serde_json::to_string(&envelope).expect("serialize invalid envelope")
    );
    let mut invalid_envelope = Cursor::new(line.into_bytes());

    assert_eq!(
        read_envelope_line(&mut invalid_envelope),
        Err(AgentCodecError::Protocol(
            ProtocolValidationError::EmptyCapabilities
        ))
    );
}

#[test]
fn write_envelope_line_rejects_invalid_envelopes() {
    let envelope = AgentEnvelope::new(AgentMessage::GuestHello {
        version: PROTOCOL_VERSION,
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: vec![],
        auth: Some(AgentAuth::ToolsToken {
            token: "token-1".to_string(),
        }),
    });
    let mut buffer = Cursor::new(Vec::new());

    assert_eq!(
        write_envelope_line(&mut buffer, &envelope),
        Err(AgentCodecError::Protocol(
            ProtocolValidationError::EmptyCapabilities
        ))
    );
    assert!(buffer.get_ref().is_empty());
}

#[test]
fn read_guest_session_accepts_first_authenticated_hello_frame() {
    let envelope = AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities()));
    let mut buffer = Cursor::new(encode_envelope_line(&envelope).unwrap().into_bytes());

    let session = read_guest_session(&mut buffer, &valid_policy()).unwrap();

    assert_eq!(session.guest_os, "linux");
    assert_eq!(session.agent_version.as_deref(), Some("1.0.0"));
    assert!(session.supports("clipboard"));
}

#[test]
fn read_guest_session_rejects_clean_eof_before_hello() {
    let mut buffer = Cursor::new(Vec::new());

    assert_eq!(
        read_guest_session(&mut buffer, &valid_policy()),
        Err(AgentSessionIoError::EofBeforeGuestHello)
    );
}

#[test]
fn read_guest_session_rejects_non_hello_first_frame() {
    let envelope = AgentEnvelope::new(AgentMessage::Heartbeat);
    let mut buffer = Cursor::new(encode_envelope_line(&envelope).unwrap().into_bytes());

    assert_eq!(
        read_guest_session(&mut buffer, &valid_policy()),
        Err(AgentSessionIoError::Agentd(AgentdError::ExpectedGuestHello))
    );
}

#[test]
fn read_guest_session_propagates_codec_errors() {
    let mut buffer = Cursor::new(br#"{"protocol_version":1}"#.to_vec());

    assert_eq!(
        read_guest_session(&mut buffer, &valid_policy()),
        Err(AgentSessionIoError::Codec(
            AgentCodecError::MissingFrameTerminator
        ))
    );
}
