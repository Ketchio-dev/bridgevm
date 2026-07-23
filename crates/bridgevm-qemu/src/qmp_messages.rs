//! QMP wire message types and the frame size limits.

use serde::Deserialize;
use serde::Serialize;
use serde_json::Value;

pub(crate) const MAX_QMP_ENVELOPE_BYTES: u64 = 1024 * 1024;

pub(crate) const MAX_QMP_SKIPPED_ENVELOPES: usize = 1024;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpEnvelope {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub greeting: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub event: Option<QmpEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpEvent {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QmpEventDrain {
    pub events: Vec<QmpEvent>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub terminal_event: Option<QmpEvent>,
    pub envelopes_read: usize,
    pub limit_reached: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QmpStatus {
    pub status: String,
    pub running: bool,
}

impl QmpEvent {
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.name.as_str(),
            "SHUTDOWN" | "RESET" | "STOP" | "GUEST_PANICKED" | "WATCHDOG"
        )
    }
}

impl QmpEventDrain {
    pub(crate) fn empty() -> Self {
        Self {
            events: Vec::new(),
            terminal_event: None,
            envelopes_read: 0,
            limit_reached: false,
        }
    }

    pub fn has_terminal_event(&self) -> bool {
        self.terminal_event.is_some()
    }
}

impl QmpStatus {
    pub fn is_terminal(&self) -> bool {
        !self.running
            && matches!(
                self.status.as_str(),
                "shutdown" | "internal-error" | "guest-panicked"
            )
    }
}
