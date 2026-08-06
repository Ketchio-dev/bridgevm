//! Typed VM lifecycle events, each stamped with the boot it belongs to.
//!
//! PLAN.md R1: stale wake/IRQ/agent events must be discarded on generation
//! mismatch. The queue enforces that structurally: publishing requires a
//! `GenerationTag` (there is no unstamped variant to publish), and `drain`
//! drops everything that is not from the current generation, counting the
//! drops so a stale burst is evidence rather than silence.

use std::collections::VecDeque;
use std::sync::Mutex;

use crate::reset_generation::{GenerationTag, ResetGeneration};

/// What happened to the VM, without device-model detail. Variants carry data
/// only when the consumer (app/runner) acts on it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VmEvent {
    /// The guest requested SYSTEM_RESET via PSCI. The product path tears the
    /// process down (flush, receipt, exit); this event is how it learns to.
    GuestRequestedReset,
    /// The guest requested SYSTEM_OFF.
    GuestRequestedPowerOff,
    /// The guest agent produced a line on its console channel.
    AgentConsoleLine(String),
    /// The vCPU stopped making boot progress; payload is the diagnostic
    /// summary line already formatted by the probe-side watchdog.
    BootProgressStall(String),
}

/// A `VmEvent` plus the boot that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StampedEvent {
    pub tag: GenerationTag,
    pub event: VmEvent,
}

/// Events drained in one call: current-generation events in publish order,
/// plus how many stale ones were discarded to get them.
#[derive(Debug, PartialEq, Eq)]
pub struct DrainedEvents {
    pub events: Vec<VmEvent>,
    pub stale_discarded: u64,
}

/// Generation-aware event queue between device threads and the runtime.
#[derive(Debug, Default)]
pub struct VmEventQueue {
    inner: Mutex<VecDeque<StampedEvent>>,
}

impl VmEventQueue {
    pub fn new() -> Self {
        Self::default()
    }

    /// Publish an event. The tag comes from `ResetGeneration::stamp` at the
    /// moment the underlying condition was observed, not at publish time:
    /// an event observed before a reset and published after it must be
    /// stamped with the old generation so `drain` can discard it.
    pub fn publish(&self, tag: GenerationTag, event: VmEvent) {
        self.inner
            .lock()
            .unwrap()
            .push_back(StampedEvent { tag, event });
    }

    /// Remove and return all current-generation events; discard and count
    /// the rest. A stale event is never returned, not even by accident of
    /// ordering: staleness is decided per event against `generation` now.
    pub fn drain(&self, generation: &ResetGeneration) -> DrainedEvents {
        let mut queue = self.inner.lock().unwrap();
        let mut events = Vec::new();
        let mut stale_discarded = 0u64;
        for stamped in queue.drain(..) {
            if generation.is_current(stamped.tag) {
                events.push(stamped.event);
            } else {
                stale_discarded += 1;
            }
        }
        DrainedEvents {
            events,
            stale_discarded,
        }
    }
}

#[cfg(test)]
#[path = "vm_event_tests.rs"]
mod tests;
