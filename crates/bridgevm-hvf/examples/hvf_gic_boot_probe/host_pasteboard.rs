// macOS pasteboard bridge for the agent-console service loop (M6-3).
//
// A dedicated thread polls NSPasteboard generalPasteboard's changeCount and
// reports external changes over an mpsc channel; set() requests (guest->host
// clipboard sync) flow the other way. The VM tick loop stays non-blocking: it
// only ever does try_recv/send on the channels.
//
// PUBLIC API (frozen — agent_console.rs codes against exactly this):
//   HostPasteboard::spawn(poll_ms) -> HostPasteboard
//   try_changed(&self) -> Option<String>  // deduped external pasteboard changes
//   set(&self, text: String)              // guest->host sync; never echoes back
//
// Echo suppression contract: text written via set() must NOT come back out of
// try_changed(), and neither must a text equal to the last one emitted/set.
// The first poll after spawn reports the pasteboard's current content (host
// wins at startup).

use std::sync::mpsc::{channel, Receiver, Sender, TryRecvError};
use std::thread;
use std::time::Duration;

/// Minimal pasteboard surface, abstracted so the poller logic is unit-testable
/// without touching the real NSPasteboard (tests must never clobber the user's
/// clipboard).
pub trait PasteboardBackend: Send {
    /// NSPasteboard changeCount — bumps on every ownership change.
    fn change_count(&mut self) -> i64;
    /// Current plain-text content (public.utf8-plain-text), if any.
    fn get_string(&mut self) -> Option<String>;
    /// Replace the pasteboard content with `text`. Returns false on failure.
    fn set_string(&mut self, text: &str) -> bool;
}

/// Pure poll/suppress state machine, generic over the backend for testability.
pub struct Poller {
    last_count: i64,
    last_seen: Option<String>,
}

impl Poller {
    pub fn new() -> Self {
        Self {
            // -1 sentinel: the first on_tick always reads the pasteboard, so
            // startup content is emitted once (host wins at startup).
            last_count: -1,
            last_seen: None,
        }
    }

    /// One poll step: Some(text) iff the pasteboard changed externally to a
    /// non-empty text we haven't already emitted or set ourselves.
    pub fn on_tick<B: PasteboardBackend>(&mut self, backend: &mut B) -> Option<String> {
        let count = backend.change_count();
        if count == self.last_count {
            return None;
        }
        self.last_count = count;
        let text = backend.get_string()?;
        if text.is_empty() || self.last_seen.as_deref() == Some(text.as_str()) {
            return None;
        }
        self.last_seen = Some(text.clone());
        Some(text)
    }

    /// Write guest text to the pasteboard, recording the post-write changeCount
    /// so the write does not re-emerge from on_tick. (If the user copies in the
    /// tiny window between set_string and change_count, that copy is missed —
    /// accepted race, the next real change re-syncs.)
    pub fn apply_set<B: PasteboardBackend>(&mut self, backend: &mut B, text: &str) {
        if backend.set_string(text) {
            self.last_seen = Some(text.to_owned());
            self.last_count = backend.change_count();
        }
    }
}

pub struct HostPasteboard {
    events: Receiver<String>,
    cmds: Sender<String>,
}

impl HostPasteboard {
    /// Spawn the polling thread. `poll_ms` is the changeCount poll interval.
    pub fn spawn(poll_ms: u64) -> Self {
        let (event_tx, event_rx) = channel();
        let (cmd_tx, cmd_rx) = channel();
        let poll = Duration::from_millis(poll_ms.max(50));
        let _ = thread::Builder::new()
            .name("bv-pasteboard".into())
            .spawn(move || run(StubBackend, poll, event_tx, cmd_rx));
        Self {
            events: event_rx,
            cmds: cmd_tx,
        }
    }

    /// Non-blocking: next external host-pasteboard change, if one is pending.
    pub fn try_changed(&self) -> Option<String> {
        self.events.try_recv().ok()
    }

    /// Queue a pasteboard write (guest->host sync). Never blocks.
    pub fn set(&self, text: String) {
        let _ = self.cmds.send(text);
    }
}

fn run<B: PasteboardBackend>(
    mut backend: B,
    poll: Duration,
    events: Sender<String>,
    cmds: Receiver<String>,
) {
    let mut poller = Poller::new();
    loop {
        // Drain set() requests first: a fresh guest->host write supersedes
        // whatever a stale poll would have reported.
        loop {
            match cmds.try_recv() {
                Ok(text) => poller.apply_set(&mut backend, &text),
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => return,
            }
        }
        if let Some(changed) = poller.on_tick(&mut backend) {
            if events.send(changed).is_err() {
                return;
            }
        }
        thread::sleep(poll);
    }
}

/// Placeholder backend so the scaffold compiles before the real NSPasteboard
/// FFI lands; reports an inert, empty pasteboard.
struct StubBackend;

impl PasteboardBackend for StubBackend {
    fn change_count(&mut self) -> i64 {
        0
    }
    fn get_string(&mut self) -> Option<String> {
        None
    }
    fn set_string(&mut self, _text: &str) -> bool {
        false
    }
}
