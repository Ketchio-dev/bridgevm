//! Split test module.

use crate::*;

pub(super) fn valid_guest_hello() -> AgentMessage {
    AgentMessage::GuestHello {
        version: PROTOCOL_VERSION,
        guest_os: "linux".to_string(),
        agent_version: Some("1.0.0".to_string()),
        capabilities: valid_capabilities(),
        auth: Some(AgentAuth::ToolsToken {
            token: "vm-token".to_string(),
        }),
    }
}

pub(super) fn valid_capabilities() -> Vec<AgentCapability> {
    vec![
        AgentCapability {
            name: "heartbeat".to_string(),
            version: 1,
        },
        AgentCapability {
            name: "clipboard".to_string(),
            version: 1,
        },
        AgentCapability {
            name: "display-resize".to_string(),
            version: 1,
        },
    ]
}
