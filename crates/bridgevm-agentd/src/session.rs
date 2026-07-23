//! AgentSession and capability lookup on a negotiated session.

use bridgevm_agent_protocol::AgentCapability;

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
