//! Mapping a message to its required capability and authorizing it against a session.

use crate::*;
use bridgevm_agent_protocol::AgentMessage;

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
