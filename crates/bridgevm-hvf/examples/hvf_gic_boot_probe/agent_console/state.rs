//! Agent console state and request model.

use super::*;

pub(super) const TEST_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST";

pub(super) const TEST_PERIODIC_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST_PERIODIC";

pub(super) const BOOT_TIMER_AGENT_ENV: &str = "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT";

pub(super) const CMDS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CMDS";

pub(super) const TIMEOUT_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS";

pub(super) const SERVICE_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SERVICE";

pub(super) const CLIPSYNC_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC";

pub(super) const CLIPSYNC_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC_MS";

pub(super) const CLIPSYNC_MAX_KB_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC_MAX_KB";

pub(super) const CTL_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CTL";

pub(super) const SHARE_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SHARE";

pub(super) const SHARE_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS";

pub(super) const SHARE_MAX_KB_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB";

pub(super) const DEFAULT_CMDS: &str = "whoami|ver|ipconfig";

pub(super) const DEFAULT_TIMEOUT_MS: u64 = 180_000;

pub(super) const DEFAULT_CLIPSYNC_MS: u64 = 1000;

pub(super) const DEFAULT_CLIPSYNC_MAX_KB: u64 = 8;

pub(super) const CLIPSYNC_MS_FLOOR: u64 = 100;

pub(super) const DEFAULT_SHARE_MS: u64 = 3000;

pub(super) const SHARE_MS_FLOOR: u64 = 500;

pub(super) const DEFAULT_SHARE_MAX_KB: u64 = 8192;

pub(super) const SHARE_PUT_CHUNK_BYTES: usize = 24 * 1024;

pub(super) const COMMAND_OUTPUT_CHUNK_BYTES: usize = 24 * 1024;

pub(super) const COMMAND_OUTPUT_CHUNK_B64_BYTES: usize = COMMAND_OUTPUT_CHUNK_BYTES.div_ceil(3) * 4;

pub(super) const MAX_COMMAND_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

pub(super) const MAX_CTL_COMMAND_BYTES: usize = 64 * 1024;

pub(super) const MAX_CTL_READ_BYTES_PER_POLL: u64 = 256 * 1024;

pub(super) const MAX_SERVICE_QUEUE_DEPTH: usize = 1024;

pub struct AgentConsoleHarness {
    pub(super) start: Instant,
    pub(super) timeout: Duration,
    /// Scripted console tests intentionally preserve their historical
    /// per-vCPU-exit polling cadence. The BOOT_TIMER-only desktop oracle uses
    /// the probe's periodic ServiceWake instead, so measurement does not add a
    /// platform-mutex acquisition to every CPU0 exit.
    pub(super) scripted_test: bool,
    /// Run scripted commands from the periodic host wake instead of taking the
    /// automation lock on every CPU0 exit. This keeps boot measurements honest
    /// while still allowing a deterministic command after agent handshake.
    pub(super) periodic_test: bool,
    pub(super) framer: LineFramer,
    pub(super) inbound_scratch: Vec<u8>,
    pub(super) line_scratch: Vec<String>,
    pub(super) state: AgentConsoleState,
    pub(super) commands: Vec<String>,
    pub(super) last_ping: Option<Instant>,
    pub(super) get_accum: Option<GetAccum>,
    pub(super) out_accum: Option<OutAccum>,
    // --- Service mode (resident host loop; see AgentConsoleState::Service). ---
    /// Stay resident after the scripted commands instead of finishing.
    pub(super) service: bool,
    /// Bidirectional clipboard auto-sync inside service mode.
    pub(super) clipsync: bool,
    /// CLIPGET poll cadence (also the pasteboard thread poll interval).
    pub(super) clip_interval: Duration,
    /// Host->guest clipboard payload ceiling (bytes); see the skip print for why.
    pub(super) clip_max_bytes: usize,
    /// Pending service requests. The guest agent is a single-threaded
    /// read-dispatch loop, so at most one request is on the wire at a time
    /// (strict lockstep; see `in_flight`).
    pub(super) queue: VecDeque<ServiceReq>,
    /// The request currently awaiting a guest reply, with the last overdue-
    /// report anchor. None means the wire is idle and the next queued request
    /// may be sent. An overdue request remains here to preserve wire alignment.
    pub(super) in_flight: Option<(ServiceReq, Instant)>,
    /// Last clipboard text synced in EITHER direction, stored normalized (LF).
    /// Guards the CRLF/LF ping-pong: a value we just pushed one way must not be
    /// re-adopted when it comes back the other way.
    pub(super) last_synced: Option<String>,
    /// macOS pasteboard bridge (guest<->host). Some only when clipsync is on.
    pub(super) pasteboard: Option<HostPasteboard>,
    /// Optional control file tailed for injected commands.
    pub(super) ctl_path: Option<String>,
    /// Byte offset consumed so far from the control file.
    pub(super) ctl_offset: u64,
    /// Line reassembly for control-file bytes (independent of the wire framer).
    pub(super) ctl_framer: LineFramer,
    /// Reused byte buffer for control-file tail reads.
    pub(super) ctl_read_scratch: Vec<u8>,
    /// Reused framed-line buffer for control-file commands.
    pub(super) ctl_line_scratch: Vec<String>,
    /// Reused host->guest service request line buffer.
    pub(super) service_line_scratch: String,
    /// Reused CRLF-normalized clipboard text for CLIPSET encoding.
    pub(super) clip_crlf_scratch: String,
    /// Reused absolute Windows guest path for shared-folder service requests.
    pub(super) share_guest_path_scratch: String,
    /// Throttle for control-file stat/reads.
    pub(super) ctl_last_poll: Option<Instant>,
    /// Last time a CLIPGET was sent (clip poll cadence).
    pub(super) last_clip_poll: Option<Instant>,
    /// Last time anything was sent to the guest (heartbeat cadence).
    pub(super) last_send: Option<Instant>,
    /// Last "SERVICE alive" heartbeat print (30s cadence in service mode).
    pub(super) last_alive: Option<Instant>,
    /// Optional shared-folder sync. Empty directories are not represented in
    /// the engine: file parent directories are created implicitly on write.
    pub(super) share: Option<ShareState>,
    pub(super) share_old_agent_warned: bool,
}

/// Reassembly state for a chunked GET reply (GETBEG -> GETCHUNK* -> GETEND).
pub(super) struct GetAccum {
    pub(super) path: String,
    pub(super) total: usize,
    pub(super) nchunks: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) chunks_seen: usize,
}

pub(super) struct FinishedGet {
    pub(super) path: String,
    pub(super) total: usize,
    pub(super) nchunks: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) chunks_seen: usize,
}

/// Reassembly state for a chunked command reply
/// (OUTBEG -> OUTCHUNK* -> OUTEND).
pub(super) struct OutAccum {
    pub(super) exit_code: i32,
    pub(super) total: usize,
    pub(super) nchunks: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) chunks_seen: usize,
    pub(super) valid: bool,
}

pub(super) struct ShareState {
    pub(super) engine: ShareSync,
    pub(super) host_dir: PathBuf,
    pub(super) guest_dir: String,
    pub(super) interval: Duration,
    pub(super) last_poll: Option<Instant>,
    pub(super) host_skip_seen: HashSet<(String, u128, HostSkipKind)>,
    pub(super) guest_ls_scratch: Vec<LsEntry>,
    pub(super) host_scan_scratch: Vec<HostFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(super) enum HostSkipKind {
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum ShareDelDirection {
    HostToGuest,
    GuestToHost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SharePutPhase {
    Legacy,
    Beg,
    Chunk,
    End,
}

pub(super) enum SharePutWireLine {
    Chunk {
        seq: usize,
        start: usize,
        end: usize,
    },
    End {
        nchunks: usize,
    },
}

pub(super) const PING_INTERVAL: Duration = Duration::from_secs(3);

/// Report a service request that has not replied after this long. The guest
/// dispatcher is single-threaded, so the request must remain in-flight: sending
/// a second request would only mislabel the eventual first reply.
pub(super) const SERVICE_OVERDUE_INTERVAL: Duration = Duration::from_secs(20);

/// With clipsync off there is no CLIPGET keeping the channel warm, so ping this
/// often when otherwise idle. With clipsync on, CLIPGET is the heartbeat.
pub(super) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// Control file is stat'd/read at most this often to keep the tick loop cheap.
pub(super) const CTL_POLL_INTERVAL: Duration = Duration::from_millis(200);

pub(super) enum AgentConsoleState {
    WaitingReady,
    WaitingPong,
    WaitingOut { index: usize },
    Service,
    Done,
    TimedOut,
}

/// A single host->guest exchange in service mode. The guest reads and dispatches
/// one line at a time, so these are issued strictly one at a time (lockstep).
#[derive(Clone)]
pub(super) enum ServiceReq {
    /// Poll the guest clipboard (CLIPGET) for guest->host sync.
    ClipPoll,
    /// Push host clipboard text to the guest (CLIPSET). Carries the host (LF)
    /// text; the CRLF conversion happens at send time.
    ClipPush(String),
    /// A command injected via the control file (same semantics as a scripted
    /// command: raw verb sent verbatim, else RUN-wrapped).
    Ctl(String),
    /// Keepalive when clipsync is off.
    Ping,
    /// Poll the guest shared directory.
    ShareLs,
    /// Pull one guest file into the host shared directory.
    ShareGet { name: String },
    /// Push one captured host file payload into the guest shared directory.
    SharePut {
        name: String,
        bytes: Vec<u8>,
        hash: u64,
        next_chunk: usize,
        phase: SharePutPhase,
    },
    /// Propagate a confirmed tombstone in one direction.
    ShareDel {
        name: String,
        direction: ShareDelDirection,
    },
}

/// Outcome of interpreting one guest reply line against the in-flight request.
pub(super) enum ReplyProgress {
    /// A multi-line reply (chunked GET) is still in progress; stay in-flight.
    Incomplete,
    /// The reply was fully handled; the request is done.
    Complete,
    /// The line didn't match this request; ignore it (stay in-flight).
    Ignored,
}

pub(super) enum InFlightSnapshot {
    ClipPoll,
    ClipPush {
        guest_bytes: usize,
    },
    Ctl(String),
    Ping,
    ShareLs,
    ShareGet {
        name: String,
    },
    SharePut,
    ShareDel {
        name: String,
        direction: ShareDelDirection,
    },
}

impl InFlightSnapshot {
    pub(super) fn from_req(req: &ServiceReq) -> Self {
        match req {
            ServiceReq::ClipPoll => Self::ClipPoll,
            ServiceReq::ClipPush(text) => Self::ClipPush {
                guest_bytes: to_guest_crlf_len(text),
            },
            ServiceReq::Ctl(command) => Self::Ctl(command.clone()),
            ServiceReq::Ping => Self::Ping,
            ServiceReq::ShareLs => Self::ShareLs,
            ServiceReq::ShareGet { name } => Self::ShareGet { name: name.clone() },
            ServiceReq::SharePut { .. } => Self::SharePut,
            ServiceReq::ShareDel { name, direction } => Self::ShareDel {
                name: name.clone(),
                direction: *direction,
            },
        }
    }
}

impl LineFramer {
    pub(super) fn new() -> Self {
        Self {
            pending: Vec::new(),
            discarding_oversized_line: false,
        }
    }
    pub(super) fn push_into(&mut self, bytes: &[u8], lines: &mut Vec<String>) {
        self.pending.extend_from_slice(bytes);

        let mut consumed = 0usize;
        while let Some(relative_newline) = self.pending[consumed..]
            .iter()
            .position(|byte| *byte == b'\n')
        {
            let newline = consumed + relative_newline;
            let line_end = if newline > consumed && self.pending[newline - 1] == b'\r' {
                newline - 1
            } else {
                newline
            };
            lines.push(String::from_utf8_lossy(&self.pending[consumed..line_end]).into_owned());
            consumed = newline + 1;
        }

        if consumed > 0 {
            self.pending.drain(..consumed);
        }
    }
    pub(super) fn push_bounded_into(
        &mut self,
        bytes: &[u8],
        lines: &mut Vec<String>,
        maximum: usize,
    ) -> u64 {
        let mut rejected = 0u64;
        for byte in bytes {
            if self.discarding_oversized_line {
                if *byte == b'\n' {
                    self.discarding_oversized_line = false;
                }
                continue;
            }
            if *byte == b'\n' {
                let line_end = if self.pending.last().is_some_and(|last| *last == b'\r') {
                    self.pending.len() - 1
                } else {
                    self.pending.len()
                };
                if line_end <= maximum {
                    lines.push(String::from_utf8_lossy(&self.pending[..line_end]).into_owned());
                } else {
                    rejected = rejected.saturating_add(1);
                }
                self.pending.clear();
            } else if self.pending.len() <= maximum {
                self.pending.push(*byte);
            } else {
                self.pending.clear();
                self.discarding_oversized_line = true;
                rejected = rejected.saturating_add(1);
            }
        }
        rejected
    }
}
