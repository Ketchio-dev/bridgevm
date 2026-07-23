//! Split out of lib.rs to keep files under 800 lines.

use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::ProtocolValidationError;
use std::collections::BTreeMap;
use std::collections::BTreeSet;
use std::io::BufRead;
use std::io::ErrorKind;
use std::io::Write;

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
pub(crate) fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
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
        | AgentMessage::CloseWindow { .. }
        | AgentMessage::SetWindowBounds { .. }
        | AgentMessage::WindowInput { .. } => Some("windows"),
        AgentMessage::GuestMetrics { .. } => Some("guest-metrics"),
        AgentMessage::FreezeFilesystem { .. } => Some("fs-freeze"),
        AgentMessage::ThawFilesystem => Some("fs-thaw"),
        AgentMessage::RunBenchmark { .. } => Some("benchmark"),
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AgentCommandTracker {
    pub(crate) pending: BTreeMap<String, PendingCommand>,
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

pub(crate) fn validate_capabilities_allowed(
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
