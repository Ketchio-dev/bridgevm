//! Every error type of the agentd host side and the conversions between them.

use bridgevm_agent_protocol::ProtocolValidationError;
use std::io::ErrorKind;

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AgentSessionIoError {
    Codec(AgentCodecError),
    Agentd(AgentdError),
    EofBeforeGuestHello,
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
