//! Guest-hello acceptance, constant-time token check, and session bootstrap.

use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::ProtocolValidationError;
use std::io::BufRead;

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

pub fn read_guest_session(
    reader: &mut impl BufRead,
    policy: &AgentPolicy,
) -> Result<AgentSession, AgentSessionIoError> {
    let envelope = read_envelope_line(reader)?.ok_or(AgentSessionIoError::EofBeforeGuestHello)?;
    accept_guest_hello(&envelope, policy).map_err(AgentSessionIoError::Agentd)
}
