//! Split test module.

use crate::*;
use bridgevm_agent_protocol::AgentAuth;
use bridgevm_agent_protocol::AgentCapability;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use bridgevm_agent_protocol::PROTOCOL_VERSION;

pub(super) fn valid_policy() -> AgentPolicy {
    AgentPolicy::new("token-1", [("clipboard", 1), ("display-resize", 1)])
}

pub(super) fn valid_session() -> AgentSession {
    accept_guest_hello(
        &AgentEnvelope::new(valid_guest_hello("token-1", valid_capabilities())),
        &valid_policy(),
    )
    .unwrap()
}

pub(super) fn valid_guest_hello(token: &str, capabilities: Vec<AgentCapability>) -> AgentMessage {
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

pub(super) fn valid_capabilities() -> Vec<AgentCapability> {
    vec![AgentCapability {
        name: "clipboard".to_string(),
        version: 1,
    }]
}
