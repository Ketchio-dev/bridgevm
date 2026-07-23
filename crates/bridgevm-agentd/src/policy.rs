//! AgentPolicy and the allowed-capability rules it enforces.

use crate::*;
use bridgevm_agent_protocol::AgentCapability;
use std::collections::BTreeMap;
use std::collections::BTreeSet;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AgentPolicy {
    pub expected_tools_token: String,
    pub allowed_capabilities: BTreeMap<String, u16>,
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
