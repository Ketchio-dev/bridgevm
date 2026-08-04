//! Fixed-size numeric event ring for SMP tracing.
//!
//! The previous tracer called `println!` at every event from inside the vCPU
//! run loops. Formatting and the stdout lock on that path changed the timing it
//! was supposed to observe: the a1-smp investigation measured a severe observer
//! effect, where enabling the trace altered the stall it was meant to explain.
//!
//! Recording here is a numeric store into a preallocated array. No allocation,
//! no formatting, no lock beyond one mutex acquisition, so the hot path cost is
//! bounded and roughly constant. Strings are produced only by `drain`, after
//! the run has stopped.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

/// Lock identities, stored as a number so the ring holds no strings.
const LOCK_PLATFORM: u64 = 0;
const LOCK_VCPU_STATE: u64 = 1;
const LOCK_OTHER: u64 = 2;

pub(crate) fn lock_id(lock_name: &str) -> u64 {
    match lock_name {
        "platform mutex" => LOCK_PLATFORM,
        "VcpuControl.state mutex" => LOCK_VCPU_STATE,
        _ => LOCK_OTHER,
    }
}

pub(crate) fn lock_name(id: u64) -> &'static str {
    match id {
        LOCK_PLATFORM => "platform mutex",
        LOCK_VCPU_STATE => "VcpuControl.state mutex",
        _ => "lock",
    }
}

pub(crate) fn psci_state_code(state: crate::hvf_abi::PsciState) -> u64 {
    match state {
        crate::hvf_abi::PsciState::Off => 0,
        crate::hvf_abi::PsciState::OnPending => 1,
        crate::hvf_abi::PsciState::On => 2,
    }
}

/// Events retained. Older events are overwritten and counted as overflow, so a
/// long run cannot grow memory without bound.
pub(crate) const RING_CAPACITY: usize = 4096;

/// What happened. Stored as a small integer, never a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum EventKind {
    StateTransition = 0,
    SecondaryCreated = 1,
    SecondaryWaitingOff = 2,
    SecondaryWoke = 3,
    RunLoopEntered = 4,
    PreRunDrain = 5,
    PostRunDrain = 6,
    RunResult = 7,
    Progress = 8,
    LockWait = 9,
    LockAcquired = 10,
}

impl EventKind {
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            EventKind::StateTransition => "state-transition",
            EventKind::SecondaryCreated => "secondary-created",
            EventKind::SecondaryWaitingOff => "secondary-waiting-off",
            EventKind::SecondaryWoke => "secondary-woke",
            EventKind::RunLoopEntered => "run-loop-entered",
            EventKind::PreRunDrain => "pre-run-drain",
            EventKind::PostRunDrain => "post-run-drain",
            EventKind::RunResult => "run-result",
            EventKind::Progress => "progress",
            EventKind::LockWait => "lock-wait",
            EventKind::LockAcquired => "lock-acquired",
        }
    }
}

/// One recorded event. Plain `Copy` numbers only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SmpEvent {
    pub(crate) kind: EventKind,
    pub(crate) cpu: u64,
    /// Meaning depends on `kind`; see the `record_*` helpers on `SmpTrace`.
    pub(crate) a: u64,
    pub(crate) b: u64,
}

impl SmpEvent {
    /// Render, resolving lock identities through the caller's name table.
    /// Never called on the hot path.
    pub(crate) fn render_with_lock_names(&self, lock_name: fn(u64) -> &'static str) -> String {
        let SmpEvent { cpu, a, b, .. } = *self;
        match self.kind {
            EventKind::LockWait => {
                format!("vCPU{cpu} waited {a} ms for {}", lock_name(b))
            }
            EventKind::LockAcquired => {
                format!("vCPU{cpu} acquired {} after {a} ms", lock_name(b))
            }
            _ => self.render(),
        }
    }

    /// Render after the run has stopped. Never called on the hot path.
    pub(crate) fn render(&self) -> String {
        let SmpEvent { cpu, a, b, .. } = *self;
        match self.kind {
            EventKind::StateTransition => {
                format!("vCPU{cpu} {} -> {}", psci_state_name(a), psci_state_name(b))
            }
            EventKind::SecondaryCreated => {
                format!("vCPU{cpu} created HVF vCPU {a} exit={b:#x}")
            }
            EventKind::SecondaryWaitingOff => format!("vCPU{cpu} blocking while Off"),
            EventKind::SecondaryWoke => {
                format!("vCPU{cpu} woke with state {}", psci_state_name(a))
            }
            EventKind::RunLoopEntered => {
                format!("vCPU{cpu} entering run loop after {a} exits")
            }
            EventKind::PreRunDrain => format!("vCPU{cpu} pre-run drain before run {a} PC={b:#x}"),
            EventKind::PostRunDrain => {
                format!("vCPU{cpu} pre-run drain complete before run {a}")
            }
            EventKind::RunResult => {
                format!("vCPU{cpu} hv_vcpu_run #{a} exit reason/status {b:#x}")
            }
            EventKind::Progress => {
                format!("progress cpu0_exits={a} secondary_exits={b}")
            }
            EventKind::LockWait => format!("vCPU{cpu} waited {a} ms for lock {b}"),
            EventKind::LockAcquired => {
                format!("vCPU{cpu} acquired lock {b} after {a} ms")
            }
        }
    }
}

/// PSCI state names, kept here so the ring stores a number rather than a
/// formatted `{:?}`.
fn psci_state_name(value: u64) -> &'static str {
    match value {
        0 => "Off",
        1 => "OnPending",
        2 => "On",
        _ => "?",
    }
}

/// A bounded ring of events plus a count of what it had to overwrite.
pub(crate) struct EventRing {
    events: Mutex<RingBuffer>,
    overflow: AtomicU64,
}

struct RingBuffer {
    /// Preallocated once; `record` never grows it.
    slots: Vec<SmpEvent>,
    next: usize,
    filled: usize,
}

impl EventRing {
    pub(crate) fn new() -> Self {
        let empty = SmpEvent {
            kind: EventKind::Progress,
            cpu: 0,
            a: 0,
            b: 0,
        };
        Self {
            events: Mutex::new(RingBuffer {
                slots: vec![empty; RING_CAPACITY],
                next: 0,
                filled: 0,
            }),
            overflow: AtomicU64::new(0),
        }
    }

    /// Store one event. Allocation-free and formatting-free.
    pub(crate) fn record(&self, event: SmpEvent) {
        // A poisoned trace mutex must not take down a vCPU thread: tracing is
        // diagnostic, so a lost event is preferable to a panic mid-run.
        let Ok(mut ring) = self.events.lock() else {
            self.overflow.fetch_add(1, Ordering::Relaxed);
            return;
        };
        if ring.filled == RING_CAPACITY {
            self.overflow.fetch_add(1, Ordering::Relaxed);
        } else {
            ring.filled += 1;
        }
        let slot = ring.next;
        ring.slots[slot] = event;
        ring.next = (slot + 1) % RING_CAPACITY;
    }

    /// Events dropped because the ring wrapped.
    pub(crate) fn overflow(&self) -> u64 {
        self.overflow.load(Ordering::Relaxed)
    }

    /// Print every retained event, plus how many were overwritten so a
    /// truncated record is never mistaken for a complete one. Call only after
    /// the run has stopped.
    pub(crate) fn dump(&self, lock_name: fn(u64) -> &'static str) {
        let events = self.drain();
        println!(
            "SMP trace: {} events retained, {} overwritten",
            events.len(),
            self.overflow()
        );
        for event in &events {
            println!(
                "SMP trace: [{}] {}",
                event.kind.as_str(),
                event.render_with_lock_names(lock_name)
            );
        }
    }

    /// Oldest-first snapshot. Call only after the run has stopped.
    pub(crate) fn drain(&self) -> Vec<SmpEvent> {
        let Ok(ring) = self.events.lock() else {
            return Vec::new();
        };
        if ring.filled < RING_CAPACITY {
            return ring.slots[..ring.filled].to_vec();
        }
        let mut out = Vec::with_capacity(RING_CAPACITY);
        out.extend_from_slice(&ring.slots[ring.next..]);
        out.extend_from_slice(&ring.slots[..ring.next]);
        out
    }
}

#[cfg(test)]
#[path = "event_ring_tests.rs"]
mod tests;
