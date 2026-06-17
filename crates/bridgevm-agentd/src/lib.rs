use bridgevm_agent_protocol::{
    AgentAuth, AgentCapability, AgentEnvelope, AgentMessage, ProtocolValidationError,
};
use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead, ErrorKind, Write};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub expected_tools_token: String,
    pub allowed_capabilities: BTreeMap<String, u16>,
}

impl AgentPolicy {
    pub fn new(
        expected_tools_token: impl Into<String>,
        allowed_capabilities: impl IntoIterator<Item = (impl Into<String>, u16)>,
    ) -> Self {
        Self {
            expected_tools_token: expected_tools_token.into(),
            allowed_capabilities: allowed_capabilities
                .into_iter()
                .map(|(name, max_version)| (name.into(), max_version))
                .collect(),
        }
    }

    pub fn validate(&self) -> Result<(), AgentdError> {
        if self.expected_tools_token.trim().is_empty() {
            return Err(AgentdError::EmptyPolicyField {
                field: "expected_tools_token",
            });
        }
        if self.allowed_capabilities.is_empty() {
            return Err(AgentdError::EmptyAllowedCapabilities);
        }
        if self
            .allowed_capabilities
            .keys()
            .any(|capability| capability.trim().is_empty())
        {
            return Err(AgentdError::EmptyPolicyField {
                field: "allowed_capabilities",
            });
        }
        if self
            .allowed_capabilities
            .values()
            .any(|max_version| *max_version == 0)
        {
            return Err(AgentdError::InvalidAllowedCapabilityVersion);
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentSession {
    pub guest_os: String,
    pub agent_version: Option<String>,
    pub capabilities: Vec<AgentCapability>,
}

impl AgentSession {
    pub fn supports(&self, capability: &str) -> bool {
        self.capabilities
            .iter()
            .any(|candidate| candidate.name == capability)
    }

    pub fn capability_version(&self, capability: &str) -> Option<u16> {
        self.capabilities
            .iter()
            .find(|candidate| candidate.name == capability)
            .map(|candidate| candidate.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentdError {
    InvalidPolicy(ProtocolValidationError),
    Protocol(ProtocolValidationError),
    ExpectedGuestHello,
    EmptyPolicyField {
        field: &'static str,
    },
    EmptyAllowedCapabilities,
    InvalidToolsToken,
    DuplicateCapability {
        capability: String,
    },
    CapabilityNotAllowed {
        capability: String,
    },
    CapabilityVersionTooNew {
        capability: String,
        max_version: u16,
        actual_version: u16,
    },
    InvalidAllowedCapabilityVersion,
    CommandNotAuthorized {
        capability: String,
    },
    ExpectedHostCommand,
    ExpectedCommandResult,
    PendingRequestExists {
        request_id: String,
    },
    UnexpectedCommandResult {
        request_id: String,
    },
}

/// Largest single newline-delimited frame the host will buffer from the agent
/// channel. Bounds host memory against a hostile guest that streams bytes
/// without a terminating newline (a sustained flood would otherwise grow the
/// read buffer until OOM). Sized to comfortably hold any legitimate frame
/// (capability list, a base64 file-drop chunk).
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentCodecError {
    EmptyFrame,
    MissingFrameTerminator,
    MultipleFrames,
    FrameTooLarge,
    Io { kind: ErrorKind, message: String },
    Json(String),
    Protocol(ProtocolValidationError),
}

impl AgentCodecError {
    pub fn is_idle_io(&self) -> bool {
        // Only a would-block / timed-out read is "idle" (no data yet -> retry).
        // UnexpectedEof means the stream was truncated/half-closed mid-frame --
        // a terminal condition the caller should reset on, not spin retrying.
        matches!(
            self,
            Self::Io { kind, .. } if matches!(kind, ErrorKind::WouldBlock | ErrorKind::TimedOut)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionIoError {
    Codec(AgentCodecError),
    Agentd(AgentdError),
    EofBeforeGuestHello,
}

impl From<AgentCodecError> for AgentSessionIoError {
    fn from(error: AgentCodecError) -> Self {
        Self::Codec(error)
    }
}

impl From<AgentdError> for AgentSessionIoError {
    fn from(error: AgentdError) -> Self {
        Self::Agentd(error)
    }
}

/// Compare two byte strings in time independent of their contents, so the
/// guest-tools token check can't be brute-forced byte-by-byte via a timing
/// side-channel. The length comparison can short-circuit (token length is fixed
/// and not secret); only the per-byte compare must be constant-time.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut diff = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        diff |= x ^ y;
    }
    diff == 0
}

pub fn accept_guest_hello(
    envelope: &AgentEnvelope,
    policy: &AgentPolicy,
) -> Result<AgentSession, AgentdError> {
    policy.validate()?;
    envelope.validate().map_err(AgentdError::Protocol)?;

    let AgentMessage::GuestHello {
        guest_os,
        agent_version,
        capabilities,
        auth,
        ..
    } = &envelope.message
    else {
        return Err(AgentdError::ExpectedGuestHello);
    };

    let Some(AgentAuth::ToolsToken { token }) = auth else {
        return Err(AgentdError::Protocol(
            ProtocolValidationError::MissingField {
                field: "guest_hello.auth",
            },
        ));
    };

    if !constant_time_eq(token.as_bytes(), policy.expected_tools_token.as_bytes()) {
        return Err(AgentdError::InvalidToolsToken);
    }

    validate_capabilities_allowed(capabilities, &policy.allowed_capabilities)?;

    Ok(AgentSession {
        guest_os: guest_os.clone(),
        agent_version: agent_version.clone(),
        capabilities: capabilities.clone(),
    })
}

pub fn encode_envelope_line(envelope: &AgentEnvelope) -> Result<String, AgentCodecError> {
    envelope.validate().map_err(AgentCodecError::Protocol)?;
    let mut line = serde_json::to_string(envelope)
        .map_err(|error| AgentCodecError::Json(error.to_string()))?;
    line.push('\n');
    Ok(line)
}

pub fn decode_envelope_line(line: &str) -> Result<AgentEnvelope, AgentCodecError> {
    if line.is_empty() {
        return Err(AgentCodecError::EmptyFrame);
    }
    if !line.ends_with('\n') {
        return Err(AgentCodecError::MissingFrameTerminator);
    }

    let frame = line.trim_end_matches('\n').trim_end_matches('\r');
    if frame.trim().is_empty() {
        return Err(AgentCodecError::EmptyFrame);
    }
    if frame.contains('\n') {
        return Err(AgentCodecError::MultipleFrames);
    }

    let envelope: AgentEnvelope =
        serde_json::from_str(frame).map_err(|error| AgentCodecError::Json(error.to_string()))?;
    envelope.validate().map_err(AgentCodecError::Protocol)?;
    Ok(envelope)
}

pub fn read_envelope_line(
    reader: &mut impl BufRead,
) -> Result<Option<AgentEnvelope>, AgentCodecError> {
    // Bounded line read (vs `read_line`, which grows without limit): accumulate
    // up to MAX_FRAME_BYTES looking for a newline, erroring out rather than
    // letting a hostile guest exhaust host memory by never sending one.
    let mut line: Vec<u8> = Vec::new();
    loop {
        let available = match reader.fill_buf() {
            Ok(buffer) => buffer,
            Err(error) => {
                return Err(AgentCodecError::Io {
                    kind: error.kind(),
                    message: error.to_string(),
                })
            }
        };
        if available.is_empty() {
            // EOF: nothing buffered -> end of stream; a partial line -> let
            // decode_envelope_line report the missing terminator.
            if line.is_empty() {
                return Ok(None);
            }
            break;
        }
        if let Some(newline) = available.iter().position(|&byte| byte == b'\n') {
            if line.len() + newline + 1 > MAX_FRAME_BYTES {
                return Err(AgentCodecError::FrameTooLarge);
            }
            line.extend_from_slice(&available[..=newline]);
            reader.consume(newline + 1);
            break;
        }
        if line.len() + available.len() > MAX_FRAME_BYTES {
            return Err(AgentCodecError::FrameTooLarge);
        }
        let consumed = available.len();
        line.extend_from_slice(available);
        reader.consume(consumed);
    }

    let line = String::from_utf8(line).map_err(|error| AgentCodecError::Io {
        kind: ErrorKind::InvalidData,
        message: error.to_string(),
    })?;
    decode_envelope_line(&line).map(Some)
}

pub fn write_envelope_line(
    writer: &mut impl Write,
    envelope: &AgentEnvelope,
) -> Result<(), AgentCodecError> {
    let line = encode_envelope_line(envelope)?;
    writer
        .write_all(line.as_bytes())
        .map_err(|error| AgentCodecError::Io {
            kind: error.kind(),
            message: error.to_string(),
        })?;
    writer.flush().map_err(|error| AgentCodecError::Io {
        kind: error.kind(),
        message: error.to_string(),
    })
}

pub fn read_guest_session(
    reader: &mut impl BufRead,
    policy: &AgentPolicy,
) -> Result<AgentSession, AgentSessionIoError> {
    let envelope = read_envelope_line(reader)?.ok_or(AgentSessionIoError::EofBeforeGuestHello)?;
    accept_guest_hello(&envelope, policy).map_err(AgentSessionIoError::Agentd)
}

pub fn authorize_message(
    session: &AgentSession,
    message: &AgentMessage,
) -> Result<(), AgentdError> {
    if let Some(capability) = required_capability(message) {
        if !session.supports(capability) {
            return Err(AgentdError::CommandNotAuthorized {
                capability: capability.to_string(),
            });
        }
    }

    Ok(())
}

pub fn required_capability(message: &AgentMessage) -> Option<&'static str> {
    match message {
        AgentMessage::GuestHello { .. }
        | AgentMessage::Heartbeat
        | AgentMessage::CommandResult { .. } => None,
        AgentMessage::AgentUpdateAvailable { .. } => Some("agent-update"),
        AgentMessage::TimeSync { .. } => Some("time-sync"),
        AgentMessage::GuestIpChanged { .. } => Some("guest-ip"),
        AgentMessage::ClipboardChanged { .. } | AgentMessage::SetClipboard { .. } => {
            Some("clipboard")
        }
        AgentMessage::ResizeDisplay { .. } => Some("display-resize"),
        AgentMessage::MountShare { .. } | AgentMessage::UnmountShare { .. } => {
            Some("shared-folders")
        }
        AgentMessage::FileDropStart { .. }
        | AgentMessage::FileDropChunk { .. }
        | AgentMessage::FileDropComplete { .. } => Some("drag-drop"),
        AgentMessage::ListApplications | AgentMessage::LaunchApplication { .. } => {
            Some("applications")
        }
        AgentMessage::ListWindows
        | AgentMessage::FocusWindow { .. }
        | AgentMessage::CloseWindow { .. } => Some("windows"),
        AgentMessage::GuestMetrics { .. } => Some("guest-metrics"),
        AgentMessage::FreezeFilesystem { .. } => Some("fs-freeze"),
        AgentMessage::ThawFilesystem => Some("fs-thaw"),
        AgentMessage::RunBenchmark { .. } => Some("benchmark"),
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AgentCommandTracker {
    pending: BTreeMap<String, PendingCommand>,
}

impl AgentCommandTracker {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn pending_count(&self) -> usize {
        self.pending.len()
    }

    pub fn has_pending(&self, request_id: &str) -> bool {
        self.pending.contains_key(request_id)
    }

    pub fn begin_host_command(
        &mut self,
        session: &AgentSession,
        envelope: &AgentEnvelope,
    ) -> Result<(), AgentdError> {
        envelope.validate().map_err(AgentdError::Protocol)?;
        if matches!(envelope.message, AgentMessage::CommandResult { .. }) {
            return Err(AgentdError::ExpectedHostCommand);
        }

        authorize_message(session, &envelope.message)?;

        if let Some(request_id) = &envelope.request_id {
            if self.pending.contains_key(request_id) {
                return Err(AgentdError::PendingRequestExists {
                    request_id: request_id.clone(),
                });
            }
            self.pending.insert(
                request_id.clone(),
                PendingCommand {
                    request_id: request_id.clone(),
                    capability: required_capability(&envelope.message).map(str::to_string),
                    message: envelope.message.clone(),
                },
            );
        }

        Ok(())
    }

    pub fn complete_command_result(
        &mut self,
        envelope: &AgentEnvelope,
    ) -> Result<PendingCommand, AgentdError> {
        envelope.validate().map_err(AgentdError::Protocol)?;
        let AgentMessage::CommandResult { request_id, .. } = &envelope.message else {
            return Err(AgentdError::ExpectedCommandResult);
        };

        self.pending
            .remove(request_id)
            .ok_or_else(|| AgentdError::UnexpectedCommandResult {
                request_id: request_id.clone(),
            })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCommand {
    pub request_id: String,
    pub capability: Option<String>,
    pub message: AgentMessage,
}

fn validate_capabilities_allowed(
    capabilities: &[AgentCapability],
    allowed: &BTreeMap<String, u16>,
) -> Result<(), AgentdError> {
    let mut seen = BTreeSet::new();
    for capability in capabilities {
        if !seen.insert(capability.name.clone()) {
            return Err(AgentdError::DuplicateCapability {
                capability: capability.name.clone(),
            });
        }
        let Some(max_version) = allowed.get(&capability.name) else {
            return Err(AgentdError::CapabilityNotAllowed {
                capability: capability.name.clone(),
            });
        };
        if capability.version > *max_version {
            return Err(AgentdError::CapabilityVersionTooNew {
                capability: capability.name.clone(),
                max_version: *max_version,
                actual_version: capability.version,
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_agent_protocol::{ProtocolValidationError, PROTOCOL_VERSION};
    use std::io::Cursor;

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

    fn valid_policy() -> AgentPolicy {
        AgentPolicy::new("token-1", [("clipboard", 1), ("display-resize", 1)])
    }

    fn valid_session() -> AgentSession {
        accept_guest_hello(
            &AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities())),
            &valid_policy(),
        )
        .unwrap()
    }

    fn valid_guest_hello(token: &str, capabilities: Vec<AgentCapability>) -> AgentMessage {
        AgentMessage::GuestHello {
            version: PROTOCOL_VERSION,
            guest_os: "linux".to_string(),
            agent_version: Some("1.0.0".to_string()),
            capabilities,
            auth: Some(AgentAuth::ToolsToken {
                token: token.to_string(),
            }),
        }
    }

    fn valid_capabilities() -> Vec<AgentCapability> {
        vec![AgentCapability {
            name: "clipboard".to_string(),
            version: 1,
        }]
    }
}
