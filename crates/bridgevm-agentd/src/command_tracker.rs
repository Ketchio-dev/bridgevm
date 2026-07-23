//! In-flight host command bookkeeping and result reconciliation.

use crate::*;
use bridgevm_agent_protocol::AgentEnvelope;
use bridgevm_agent_protocol::AgentMessage;
use std::collections::BTreeMap;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct AgentCommandTracker {
    pub(crate) pending: BTreeMap<String, PendingCommand>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingCommand {
    pub request_id: String,
    pub capability: Option<String>,
    pub message: AgentMessage,
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
