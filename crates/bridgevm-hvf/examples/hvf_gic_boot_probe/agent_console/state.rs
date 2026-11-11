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

/// The largest Mac copy pushed to Windows: the same 8 MiB a Windows copy may
/// bring to the Mac. (It was 8 KiB, for an agent whose line reader a huge
/// CLIPSET wedged; an 8 MB copy now lands in under a second.)
pub(super) const DEFAULT_CLIPSYNC_MAX_KB: u64 = (super::clip_guest_admission::GUEST_CLIP_MAX_BYTES / 1024) as u64;

pub(super) const CLIPSYNC_MS_FLOOR: u64 = 100;
/// How often the pasteboard thread checks the Mac's pasteboard (its change
/// count, under a microsecond) and applies guest copies to it. Faster than the
/// guest poll: each tick of either waited a whole CLIPSYNC interval before.
pub(super) const HOST_PASTEBOARD_POLL_MS: u64 = 100;

pub(super) const DEFAULT_SHARE_MS: u64 = 3000;

pub(super) const SHARE_MS_FLOOR: u64 = 500;

pub(super) const DEFAULT_SHARE_MAX_KB: u64 = 8192;

pub(super) const SHARE_PUT_CHUNK_BYTES: usize = 24 * 1024;

/// Raw bytes per PUTDATA chunk, for agents whose READY carries `putdata`: as a
/// Finder drop's chunk, read and decoded in compiled code (share_put.rs).
pub(super) const SHARE_PUT_DATA_CHUNK_BYTES: usize = 1024 * 1024;

pub(super) const COMMAND_OUTPUT_CHUNK_BYTES: usize = 24 * 1024;

pub(super) const COMMAND_OUTPUT_CHUNK_B64_BYTES: usize = COMMAND_OUTPUT_CHUNK_BYTES.div_ceil(3) * 4;

pub(super) const MAX_COMMAND_OUTPUT_BYTES: usize = 16 * 1024 * 1024;

/// Equals the app's input-line guard: input verb, id, base64 of one 65,536-byte text event.
pub(super) const MAX_CTL_COMMAND_BYTES: usize = 87_434;
pub(super) const MAX_CTL_READ_BYTES_PER_POLL: u64 = 256 * 1024;

pub(super) const MAX_SERVICE_QUEUE_DEPTH: usize = 1024;

pub struct AgentConsoleHarness {
    pub(super) start: Instant,
    pub(super) timeout: Duration,
    /// Scripted console tests intentionally preserve their historical
    /// per-vCPU-exit polling cadence. The BOOT_TIMER-only desktop signal uses
    /// the probe's periodic ServiceWake instead, so measurement does not add a
    /// platform-mutex acquisition to every CPU0 exit.
    pub(super) scripted_test: bool,
    /// Run scripted commands from the service thread's own tick instead of
    /// taking the automation lock on every CPU0 exit. This keeps boot measurements honest
    /// while still allowing a deterministic command after agent handshake.
    pub(super) periodic_test: bool,
    pub(super) framer: LineFramer,
    pub(super) inbound_scratch: Vec<u8>,
    pub(super) line_scratch: Vec<String>,
    pub(super) state: AgentConsoleState,
    pub(super) commands: Vec<String>,
    pub(super) last_ping: Option<Instant>,
    /// When the scripted handshake command in flight went out.
    pub(super) out_since: Option<Instant>,
    pub(super) get_accum: Option<GetAccum>,
    pub(super) out_accum: Option<OutAccum>,
    // --- Service mode (resident host loop; see AgentConsoleState::Service). ---
    /// Stay resident after the scripted commands instead of finishing.
    pub(super) service: bool,
    /// Bidirectional clipboard auto-sync inside service mode.
    pub(super) clipsync: bool,
    /// CLIPGET poll cadence.
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
    /// Consecutive overdue reports for the current in-flight request; at 3
    /// the request is dropped from the wire and re-queued (retransmit).
    pub(super) overdue_beats: u32,
    /// Clipboard text the guest is known to hold (normalized LF); advances to
    /// a host copy only on CLIPSET OK. Guards the CRLF/LF ping-pong and stale
    /// guest text replacing a newer Mac copy (see clip_sync.rs).
    pub(super) last_synced: ClipBaseline,
    /// macOS pasteboard bridge (guest<->host). Some only when clipsync is on.
    pub(super) pasteboard: Option<HostPasteboard>,
    /// Clipboard images: Some once the agent's CLIPCAPS reply carried
    /// IMAGE_PNG and its clipboard directory (clip_image.rs).
    pub(super) clip_images: Option<ClipImages>,
    /// The agent's CLIPCAPS reply carried CLIPPUSH: it writes CLIPCHANGED after
    /// each guest clipboard change, so the clipboard poll is only a 10 s safety
    /// net and keepalive.
    pub(super) clip_push: bool,
    /// The agent's READY carried `putdata`: shared-folder chunks go as PUTDATA.
    pub(super) put_data: bool,
    /// The agent's READY carried `bigfile`: large shared-folder files move in ranges (share_big.rs).
    pub(super) share_big: bool,
    /// The agent refused DISPLAYMODE or DISPLAYSCALE since its READY: display requests go as
    /// RUN powershell lines (display_verbs.rs).
    pub(super) display_verbs_refused: bool,
    /// The app sent LINKWATCH to this harness's agent: a restarted agent gets it again
    /// (guest_link.rs).
    pub(super) link_watch: bool,
    /// The agent answered CLIPCAPS with CF_HTML (and IMAGE_PNG): rich text moves
    /// as CLIPXH polls and CLIPSETH pushes (clip_html.rs).
    pub(super) clip_html: bool,
    /// The agent answered CLIPCAPS with CF_HDROP (and CF_HTML): Explorer file
    /// copies come as CLIPXF polls (clip_files.rs).
    pub(super) clip_files: bool,
    /// The agent also answered CLIPSETFILES: Finder file copies go to Windows
    /// (clip_files_push.rs).
    pub(super) clip_files_push: bool,
    /// Where guest file copies are staged, and the transfer under way.
    pub(super) clip_files_stage: ClipFilesStage,
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
    /// Host suspension detection for the guest clock resync.
    pub(super) clock: HostClock,
    /// NAT resolver switches, for the guest DNS cache flush.
    pub(super) dns_flush: DnsFlush,
    /// Finder drops copied into the guest Desktop (file_drop.rs).
    pub(super) drops: FileDrops,
    /// Installs the app's copy of the guest agent over an older one.
    pub(super) agent_update: Option<AgentUpdate>,
}

/// Reassembly state for a chunked GET reply (GETBEG -> GETCHUNK* -> GETEND).
pub(super) struct GetAccum {
    pub(super) path: String,
    pub(super) total: usize,
    pub(super) nchunks: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) chunks_seen: usize,
    /// Where a GETRANGE reply sits in its file.
    pub(super) range: Option<GetRange>,
}

pub(super) struct FinishedGet {
    pub(super) path: String,
    pub(super) total: usize,
    pub(super) nchunks: usize,
    pub(super) bytes: Vec<u8>,
    pub(super) chunks_seen: usize,
    pub(super) range: Option<GetRange>,
}

/// A GETRANGE reply's GETBEG tail: the range's offset, and the file's size and time after the read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct GetRange {
    pub(super) offset: u64,
    pub(super) size: u64,
    pub(super) mtime: String,
}

impl GetRange {
    /// The reply holds the whole file.
    pub(super) fn whole(&self, total: usize) -> bool {
        self.offset == 0 && self.size == total as u64
    }
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

#[path = "share/share_state.rs"]
mod share_state;
pub(super) use share_state::ShareState;
pub(super) use share_state::host_watch::HostWatch;
pub(super) use share_state::sync_file;

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
/// A resident-service handshake command (whoami) with no reply this long: the
/// agent restarted without a READY we heard, so the handshake starts again.
pub(super) const HANDSHAKE_REPLY_TIMEOUT: Duration = Duration::from_secs(60);

/// Report a service request that has not replied after this long. The guest
/// dispatcher is single-threaded, so the request must remain in-flight: sending
/// a second request would only mislabel the eventual first reply.
pub(super) const SERVICE_OVERDUE_INTERVAL: Duration = Duration::from_secs(20);

/// With clipsync off there is no CLIPGET keeping the channel warm, so ping this
/// often when otherwise idle. With clipsync on, CLIPGET is the heartbeat.
pub(super) const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);

/// Fallback cadence for reading the control file; ctl_wake.rs reads it at once
/// on an append.
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
    /// text; the CRLF conversion happens at send time. With the base64 of the
    /// copy's CF_HTML (rich text on), the push is a CLIPSETH.
    ClipPush(String, Option<String>),
    /// Ask the agent whether it can move clipboard images (CLIPCAPS).
    ClipCaps,
    /// Push a Mac image, with the copy's text, to the guest clipboard.
    ClipImagePush(ClipImagePush),
    /// Fetch the guest clipboard image a CLIPX reply announced.
    /// `owner`: the Mac pasteboard when the fetch was queued; any Mac copy since wins.
    ClipImagePull { digest: [u8; 32], bytes: usize, owner: Option<host_pasteboard::PasteboardOwner> },
    /// Fetch the next file of the guest file copy under way (clip_files.rs).
    ClipFilesPull,
    /// Push a Mac file copy, with its names, to the guest clipboard.
    ClipFilesPush(ClipFilesPush),
    /// A command injected via the control file (same semantics as a scripted
    /// command: raw verb sent verbatim, else RUN-wrapped).
    Ctl(String),
    /// Keepalive when clipsync is off.
    Ping,
    /// Poll the guest shared directory.
    ShareLs,
    /// Pull one guest file into the host shared directory.
    ShareGet { name: String },
    /// The next range of a large guest file being copied (share_big.rs).
    ShareRange { name: String, offset: u64 },
    /// One step of a large Mac file streamed to the guest (share_big_put.rs).
    ShareBigPut(BigPutStep),
    /// Push one captured host file payload into the guest shared directory.
    SharePut {
        name: String,
        bytes: Vec<u8>,
        hash: u64,
        /// The Mac file's modification time, which the guest copy keeps.
        mtime_ms: Option<u128>,
        next_chunk: usize,
        phase: SharePutPhase,
    },
    /// Propagate a confirmed tombstone in one direction.
    ShareDel {
        name: String,
        direction: ShareDelDirection,
    },
    /// Set the guest clock to host UTC after a host suspension.
    TimeSync,
    /// One step of a Finder drop (file_drop.rs).
    Drop(DropReq),
    /// One step of a guest tools update (agent_update.rs).
    AgentUpdate(UpdateStep),
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
    ClipCaps,
    ClipImagePush,
    ClipImagePull,
    ClipFilesPull,
    ClipFilesPush,
    Ctl(String),
    Ping,
    ShareLs,
    ShareGet {
        name: String,
    },
    SharePut,
    ShareBigPut,
    ShareDel {
        name: String,
        direction: ShareDelDirection,
    },
    TimeSync,
    Drop,
    AgentUpdate,
}

impl InFlightSnapshot {
    pub(super) fn from_req(req: &ServiceReq) -> Self {
        match req {
            ServiceReq::ClipPoll => Self::ClipPoll,
            ServiceReq::ClipPush(text, _) => Self::ClipPush {
                guest_bytes: to_guest_crlf_len(text),
            },
            ServiceReq::ClipCaps => Self::ClipCaps,
            ServiceReq::ClipImagePush(_) => Self::ClipImagePush,
            ServiceReq::ClipImagePull { .. } => Self::ClipImagePull,
            ServiceReq::ClipFilesPull => Self::ClipFilesPull,
            ServiceReq::ClipFilesPush(_) => Self::ClipFilesPush,
            ServiceReq::Ctl(command) => Self::Ctl(command.clone()),
            ServiceReq::Ping => Self::Ping,
            ServiceReq::ShareLs => Self::ShareLs,
            ServiceReq::ShareGet { name } | ServiceReq::ShareRange { name, .. } => {
                Self::ShareGet { name: name.clone() }
            }
            ServiceReq::SharePut { .. } => Self::SharePut,
            ServiceReq::ShareBigPut(_) => Self::ShareBigPut,
            ServiceReq::ShareDel { name, direction } => Self::ShareDel {
                name: name.clone(),
                direction: *direction,
            },
            ServiceReq::TimeSync => Self::TimeSync,
            ServiceReq::Drop(_) => Self::Drop,
            ServiceReq::AgentUpdate(_) => Self::AgentUpdate,
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
    #[cfg(test)]
    pub(super) fn push_into(&mut self, bytes: &[u8], lines: &mut Vec<String>) {
        self.push_bounded_into(bytes, lines, usize::MAX);
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
