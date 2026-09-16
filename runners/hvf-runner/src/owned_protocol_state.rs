//! Admission commits identity/cause before the runtime observes cancellation.
use super::dto::{Ack, Event, Stop};
use std::sync::{mpsc::SyncSender, Arc, Mutex};
use std::time::Instant;
#[derive(Default)]
pub(super) struct State {
    pub cause: Option<&'static str>,
    pub operation: Option<String>,
    pub finishing: Option<Instant>,
    pub broken: bool,
}
pub(super) type Shared = Arc<Mutex<State>>;
impl State {
    pub fn latch(&mut self, cause: &'static str) {
        self.cause.get_or_insert(cause);
    }
    pub fn broken(&mut self) {
        self.broken = true;
        self.latch("protocolFailed");
    }
    pub fn stop(&mut self, stop: Stop, sender: &SyncSender<Event>) {
        if self
            .operation
            .as_ref()
            .is_some_and(|id| id != &stop.operation_id)
        {
            self.broken();
            return;
        }
        if self.operation.is_some() || self.finishing.is_some() {
            return;
        }
        self.operation = Some(stop.operation_id.clone());
        self.latch("stopRequested");
        let mut event = Event::new("stopAck");
        event.stop_ack = Some(Ack {
            operation_id: stop.operation_id,
            disposition: "accepted".into(),
        });
        if sender.try_send(event).is_err() {
            self.broken();
        }
    }
}

#[cfg(test)]
#[path = "owned_protocol_state_tests.rs"]
mod tests;
