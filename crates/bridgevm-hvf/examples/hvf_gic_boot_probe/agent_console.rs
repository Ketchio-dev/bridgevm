use std::collections::{HashSet, VecDeque};
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;

use super::{hv_vcpus_exit, HvVcpuT, EXIT_CANCELED};

#[path = "host_pasteboard.rs"]
mod host_pasteboard;
#[path = "share_sync.rs"]
mod share_sync;

use host_pasteboard::HostPasteboard;
use share_sync::{GuestFileOutcome, HostFile, LsEntry, ShareSync, SkipReason, SyncAction};

const TEST_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST";
const BOOT_TIMER_AGENT_ENV: &str = "BRIDGEVM_BOOT_TIMER_DESKTOP_AGENT";
const CMDS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CMDS";
const TIMEOUT_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS";
const SERVICE_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SERVICE";
const CLIPSYNC_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC";
const CLIPSYNC_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC_MS";
const CLIPSYNC_MAX_KB_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC_MAX_KB";
const CTL_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CTL";
const SHARE_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SHARE";
const SHARE_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS";
const SHARE_MAX_KB_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB";
const DEFAULT_CMDS: &str = "whoami|ver|ipconfig";
const DEFAULT_TIMEOUT_MS: u64 = 180_000;
const DEFAULT_CLIPSYNC_MS: u64 = 1000;
const DEFAULT_CLIPSYNC_MAX_KB: u64 = 8;
const CLIPSYNC_MS_FLOOR: u64 = 100;
const DEFAULT_SHARE_MS: u64 = 3000;
const SHARE_MS_FLOOR: u64 = 500;
const DEFAULT_SHARE_MAX_KB: u64 = 8192;
const SHARE_PUT_CHUNK_BYTES: usize = 24 * 1024;

pub struct AgentConsoleHarness {
    start: Instant,
    timeout: Duration,
    /// Scripted console tests intentionally preserve their historical
    /// per-vCPU-exit polling cadence. The BOOT_TIMER-only desktop oracle uses
    /// the probe's periodic ServiceWake instead, so measurement does not add a
    /// platform-mutex acquisition to every CPU0 exit.
    scripted_test: bool,
    framer: LineFramer,
    inbound_scratch: Vec<u8>,
    line_scratch: Vec<String>,
    state: AgentConsoleState,
    commands: Vec<String>,
    last_ping: Option<Instant>,
    get_accum: Option<GetAccum>,
    // --- Service mode (resident host loop; see AgentConsoleState::Service). ---
    /// Stay resident after the scripted commands instead of finishing.
    service: bool,
    /// Bidirectional clipboard auto-sync inside service mode.
    clipsync: bool,
    /// CLIPGET poll cadence (also the pasteboard thread poll interval).
    clip_interval: Duration,
    /// Host->guest clipboard payload ceiling (bytes); see the skip print for why.
    clip_max_bytes: usize,
    /// Pending service requests. The guest agent is a single-threaded
    /// read-dispatch loop, so at most one request is on the wire at a time
    /// (strict lockstep; see `in_flight`).
    queue: VecDeque<ServiceReq>,
    /// The request currently awaiting a guest reply, with its send time (for the
    /// stall timeout). None means the wire is idle and the next queued request
    /// may be sent.
    in_flight: Option<(ServiceReq, Instant)>,
    /// Last clipboard text synced in EITHER direction, stored normalized (LF).
    /// Guards the CRLF/LF ping-pong: a value we just pushed one way must not be
    /// re-adopted when it comes back the other way.
    last_synced: Option<String>,
    /// macOS pasteboard bridge (guest<->host). Some only when clipsync is on.
    pasteboard: Option<HostPasteboard>,
    /// Optional control file tailed for injected commands.
    ctl_path: Option<String>,
    /// Byte offset consumed so far from the control file.
    ctl_offset: u64,
    /// Line reassembly for control-file bytes (independent of the wire framer).
    ctl_framer: LineFramer,
    /// Reused byte buffer for control-file tail reads.
    ctl_read_scratch: Vec<u8>,
    /// Reused framed-line buffer for control-file commands.
    ctl_line_scratch: Vec<String>,
    /// Reused host->guest service request line buffer.
    service_line_scratch: String,
    /// Reused CRLF-normalized clipboard text for CLIPSET encoding.
    clip_crlf_scratch: String,
    /// Reused absolute Windows guest path for shared-folder service requests.
    share_guest_path_scratch: String,
    /// Throttle for control-file stat/reads.
    ctl_last_poll: Option<Instant>,
    /// Last time a CLIPGET was sent (clip poll cadence).
    last_clip_poll: Option<Instant>,
    /// Last time anything was sent to the guest (heartbeat cadence).
    last_send: Option<Instant>,
    /// Last "SERVICE alive" heartbeat print (30s cadence in service mode).
    last_alive: Option<Instant>,
    /// Optional shared-folder sync. Empty directories are not represented in
    /// the engine: file parent directories are created implicitly on write.
    share: Option<ShareState>,
    share_old_agent_warned: bool,
}

/// Reassembly state for a chunked GET reply (GETBEG -> GETCHUNK* -> GETEND).
struct GetAccum {
    path: String,
    total: usize,
    nchunks: usize,
    bytes: Vec<u8>,
    chunks_seen: usize,
}

struct FinishedGet {
    path: String,
    total: usize,
    nchunks: usize,
    bytes: Vec<u8>,
    chunks_seen: usize,
}

struct ShareState {
    engine: ShareSync,
    host_dir: PathBuf,
    guest_dir: String,
    interval: Duration,
    last_poll: Option<Instant>,
    host_skip_seen: HashSet<(String, u128, HostSkipKind)>,
    guest_ls_scratch: Vec<LsEntry>,
    host_scan_scratch: Vec<HostFile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HostSkipKind {
    TooLarge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShareDelDirection {
    HostToGuest,
    GuestToHost,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SharePutPhase {
    Legacy,
    Beg,
    Chunk,
    End,
}

enum SharePutWireLine {
    Chunk {
        seq: usize,
        start: usize,
        end: usize,
    },
    End {
        nchunks: usize,
    },
}

const PING_INTERVAL: Duration = Duration::from_secs(3);
/// Drop a service request whose reply never arrives after this long (agent
/// wedged or mid-restart), so a single lost reply can't freeze the queue.
const SERVICE_TIMEOUT: Duration = Duration::from_secs(20);
/// With clipsync off there is no CLIPGET keeping the channel warm, so ping this
/// often when otherwise idle. With clipsync on, CLIPGET is the heartbeat.
const HEARTBEAT_INTERVAL: Duration = Duration::from_secs(10);
/// Control file is stat'd/read at most this often to keep the tick loop cheap.
const CTL_POLL_INTERVAL: Duration = Duration::from_millis(200);

enum AgentConsoleState {
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
enum ServiceReq {
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
enum ReplyProgress {
    /// A multi-line reply (chunked GET) is still in progress; stay in-flight.
    Incomplete,
    /// The reply was fully handled; the request is done.
    Complete,
    /// The line didn't match this request; ignore it (stay in-flight).
    Ignored,
}

enum InFlightSnapshot {
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
    fn from_req(req: &ServiceReq) -> Self {
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

impl AgentConsoleHarness {
    /// Milliseconds since boot for the BVAGENT evidence prints, so live-run
    /// latencies are measurable straight from run.log.
    fn t_ms(&self, now: Instant) -> u128 {
        now.duration_since(self.start).as_millis()
    }

    /// Whether the main loop should run the ServiceWake ticker: resident
    /// service mode is host-driven, so it must not depend on guest activity
    /// for its tick cadence.
    pub fn service_wake_needed(&self) -> bool {
        self.service
    }

    /// Whether this harness must retain the legacy every-exit automation tick.
    /// Agent-only BOOT_TIMER runs are driven by ServiceWake and return false.
    pub const fn per_exit_tick_needed(&self) -> bool {
        self.scripted_test
    }

    /// A READY hello or proactive PONG is emitted by the logon agent, making
    /// it a stable desktop oracle that is not invalidated by clock pixels.
    pub fn desktop_ready(&self) -> bool {
        !matches!(
            self.state,
            AgentConsoleState::WaitingReady | AgentConsoleState::TimedOut
        )
    }

    pub fn from_env(start: Instant) -> Option<Self> {
        let scripted_test = env_flag(TEST_ENV);
        let boot_timer_agent = env_flag(BOOT_TIMER_AGENT_ENV);
        if !scripted_test && !boot_timer_agent {
            return None;
        }
        let service = env_flag(SERVICE_ENV);
        // Clipboard auto-sync only makes sense inside the resident service loop.
        let clipsync = service && env_flag(CLIPSYNC_ENV);
        let clip_ms = env_u64(CLIPSYNC_MS_ENV, DEFAULT_CLIPSYNC_MS).max(CLIPSYNC_MS_FLOOR);
        // The one cadence drives both the pasteboard poll thread and our CLIPGET
        // rate, so host and guest are sampled in step.
        let pasteboard = clipsync.then(|| HostPasteboard::spawn(clip_ms));
        let ctl_path = std::env::var(CTL_ENV).ok().filter(|s| !s.is_empty());
        // Start the ctl tail at the file's CURRENT end, not offset 0: the
        // harness is re-created per boot generation, and re-ingesting old ctl
        // content replayed already-executed commands after every guest reboot
        // (live-observed: the PUT and the `shutdown /r` that caused the reboot
        // ran again — a self-sustaining reboot loop in the worst case).
        let ctl_offset = ctl_path
            .as_deref()
            .and_then(|p| std::fs::metadata(p).ok())
            .map(|m| m.len())
            .unwrap_or(0);
        let share = service.then(init_share_from_env).flatten();
        Some(Self {
            start,
            timeout: Duration::from_millis(env_u64(TIMEOUT_MS_ENV, DEFAULT_TIMEOUT_MS)),
            scripted_test,
            framer: LineFramer::new(),
            inbound_scratch: Vec::new(),
            line_scratch: Vec::new(),
            state: AgentConsoleState::WaitingReady,
            commands: if scripted_test {
                agent_commands_from_env()
            } else {
                Vec::new()
            },
            last_ping: None,
            get_accum: None,
            service,
            clipsync,
            clip_interval: Duration::from_millis(clip_ms),
            clip_max_bytes: (env_u64(CLIPSYNC_MAX_KB_ENV, DEFAULT_CLIPSYNC_MAX_KB).max(1) * 1024)
                as usize,
            queue: VecDeque::new(),
            in_flight: None,
            last_synced: None,
            pasteboard,
            ctl_path,
            ctl_offset,
            ctl_framer: LineFramer::new(),
            ctl_read_scratch: Vec::new(),
            ctl_line_scratch: Vec::new(),
            service_line_scratch: String::new(),
            clip_crlf_scratch: String::new(),
            share_guest_path_scratch: String::new(),
            ctl_last_poll: None,
            last_clip_poll: None,
            last_send: None,
            last_alive: None,
            share,
            share_old_agent_warned: false,
        })
    }

    pub fn tick(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        if matches!(
            self.state,
            AgentConsoleState::Done | AgentConsoleState::TimedOut
        ) {
            return;
        }

        self.inbound_scratch.clear();
        platform.virtio_console_agent_drain_inbound_into(&mut self.inbound_scratch);
        self.line_scratch.clear();
        self.framer
            .push_into(&self.inbound_scratch, &mut self.line_scratch);
        self.inbound_scratch.clear();
        let mut lines = std::mem::take(&mut self.line_scratch);
        for line in lines.drain(..) {
            self.handle_line(&line, platform, mem, now);
        }
        self.line_scratch = lines;

        // Proactively PING while waiting to connect. The agent may have sent
        // its one-shot READY before the guest driver's host-open latched (that
        // write is silently dropped), so waiting passively for READY can
        // deadlock. A PING the agent echoes as PONG breaks that: any inbound
        // line (READY or PONG) advances us to the command phase.
        if matches!(self.state, AgentConsoleState::WaitingReady) {
            let due = self
                .last_ping
                .is_none_or(|t| now.duration_since(t) >= PING_INTERVAL);
            if due {
                platform.virtio_console_agent_send(b"PING\n", mem);
                self.last_ping = Some(now);
            }
        }

        if matches!(
            self.state,
            AgentConsoleState::WaitingReady | AgentConsoleState::WaitingPong
        ) && now.duration_since(self.start) >= self.timeout
        {
            println!("BVAGENT TIMEOUT waiting for READY");
            self.state = AgentConsoleState::TimedOut;
        }

        // Once connected and out of the scripted phase, run the resident loop:
        // pump the request queue and the clipboard bridge.
        if matches!(self.state, AgentConsoleState::Service) {
            self.service_tick(platform, mem, now);
        }

        // BVAGENT lines are the greppable live evidence, and run.log is a shell
        // redirect (not a tty) — live-observed to lag by minutes without an
        // explicit flush, which breaks log-tailing proof drivers. Flushing an
        // empty buffer is a no-op, so doing it every tick is cheap.
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }

    fn handle_line(
        &mut self,
        line: &str,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        match self.state {
            AgentConsoleState::WaitingReady => {
                // Connect on READY (agent hello) OR PONG (reply to a proactive
                // PING when READY was lost). Either proves the channel is live.
                if let Some(hostname) = line.strip_prefix("READY ") {
                    println!(
                        "BVAGENT READY host={} t={}",
                        hostname,
                        now.duration_since(self.start).as_millis()
                    );
                    platform.virtio_console_agent_send(b"PING\n", mem);
                    self.state = AgentConsoleState::WaitingPong;
                } else if line == "PONG" {
                    println!(
                        "BVAGENT PONG (proactive) t={}",
                        now.duration_since(self.start).as_millis()
                    );
                    self.send_next_command_or_done(0, platform, mem, now);
                }
            }
            AgentConsoleState::WaitingPong => {
                if line != "PONG" {
                    return;
                }
                println!("BVAGENT PONG");
                self.send_next_command_or_done(0, platform, mem, now);
            }
            AgentConsoleState::WaitingOut { index } => {
                // Label the reply with the scripted command it answers. A
                // complete single-line reply advances to the next command; a
                // chunked GET stays in-flight until GETEND; a stray line is
                // ignored.
                let progress = if let Some(progress) = self.handle_unlabelled_get_fragment(line) {
                    progress
                } else {
                    let command = self.commands[index].clone();
                    self.handle_reply_line(line, &command)
                };
                match progress {
                    ReplyProgress::Complete => {
                        self.send_next_command_or_done(index + 1, platform, mem, now);
                    }
                    ReplyProgress::Incomplete | ReplyProgress::Ignored => {}
                }
            }
            AgentConsoleState::Service => {
                self.handle_service_reply(line, Some(platform), Some(mem), now);
            }
            AgentConsoleState::Done | AgentConsoleState::TimedOut => {}
        }
    }

    /// Interpret one guest reply line against the command currently in-flight
    /// (labelled by `command`), printing the exact "BVAGENT ..." lines scripted
    /// mode has always produced. Shared by scripted WaitingOut replies and by
    /// service-mode Ctl replies so both render identically.
    ///
    /// A chunked GET (GETBEG -> GETCHUNK* -> GETEND) spans several lines and
    /// stays Incomplete until GETEND; every other reply is single-line. A
    /// command's terminal reply is one of: OUT <exit> <b64> (RUN/PS), LSOK <b64>
    /// (LS), PUTOK <b64(path)> <bytes> (PUT), CLIP <b64> (CLIPGET), or
    /// OK <...> / ERR <...> (CLIPSET & other verbs). Anything else is stray.
    fn handle_reply_line(&mut self, line: &str, command: &str) -> ReplyProgress {
        if let Some(progress) = self.handle_unlabelled_get_fragment(line) {
            return progress;
        }
        if let Some(rest) = line.strip_prefix("GETEND ") {
            self.finish_get(command, rest);
            return ReplyProgress::Complete;
        }
        if let Some(b64) = line.strip_prefix("LSOK ") {
            match base64_decode(b64) {
                Ok(bytes) => println!(
                    "BVAGENT LS {command}\n{}BVAGENT END {command}",
                    String::from_utf8_lossy(&bytes)
                ),
                Err(error) => println!(
                    "BVAGENT LS {command}\n<base64 decode error: {error:?}>\nBVAGENT END {command}"
                ),
            }
            return ReplyProgress::Complete;
        }
        if let Some(rest) = line.strip_prefix("PUTOK ") {
            // PUTOK <b64(path)> <bytes>
            let (path_b64, written) = rest.split_once(' ').unwrap_or((rest, "?"));
            let path = base64_decode(path_b64)
                .map(|b| String::from_utf8_lossy(&b).into_owned())
                .unwrap_or_else(|_| "<?>".into());
            println!("BVAGENT PUT {command} -> path={path} bytes={written}");
            return ReplyProgress::Complete;
        }
        if let Some((exit_code, output)) = parse_out_line(line) {
            match base64_decode(output) {
                Ok(bytes) => {
                    let text = String::from_utf8_lossy(&bytes);
                    println!(
                        "BVAGENT CMD {command} exit={exit_code}\n{text}\nBVAGENT END {command}"
                    );
                }
                Err(error) => {
                    println!(
                        "BVAGENT CMD {command} exit={exit_code}\n<base64 decode error: {error:?}>\nBVAGENT END {command}"
                    );
                }
            }
            return ReplyProgress::Complete;
        }
        if let Some(b64) = line.strip_prefix("CLIP ") {
            match base64_decode(b64) {
                Ok(bytes) => println!(
                    "BVAGENT CLIP {command}\n{}\nBVAGENT END {command}",
                    String::from_utf8_lossy(&bytes)
                ),
                Err(error) => println!(
                    "BVAGENT CLIP {command}\n<base64 decode error: {error:?}>\nBVAGENT END {command}"
                ),
            }
            return ReplyProgress::Complete;
        }
        if line.starts_with("OK") || line.starts_with("ERR") {
            println!("BVAGENT {command} -> {line}");
            return ReplyProgress::Complete;
        }
        ReplyProgress::Ignored
    }

    fn handle_unlabelled_get_fragment(&mut self, line: &str) -> Option<ReplyProgress> {
        if let Some(rest) = line.strip_prefix("GETBEG ") {
            self.begin_get(rest);
            return Some(ReplyProgress::Incomplete);
        }
        if let Some(rest) = line.strip_prefix("GETCHUNK ") {
            self.accum_get_chunk(rest);
            return Some(ReplyProgress::Incomplete);
        }
        None
    }

    fn send_next_command_or_done(
        &mut self,
        index: usize,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        if let Some(line) = self.next_command_line(index, now) {
            platform.virtio_console_agent_send(line.as_bytes(), mem);
        }
    }

    /// Advance to command `index`: returns the wire bytes to send and moves to
    /// WaitingOut, or None when the list is exhausted — transitioning to the
    /// resident Service state (if enabled) or Done. Split from the send so the
    /// terminal transition is unit-testable without a platform handle.
    fn next_command_line(&mut self, index: usize, now: Instant) -> Option<String> {
        let Some(command) = self.commands.get(index) else {
            if self.service {
                println!("BVAGENT SERVICE start t={}", self.t_ms(now));
                // Anchor the heartbeat clock at service entry so the first ping
                // is a full interval away rather than immediate.
                self.last_send = Some(now);
                self.state = AgentConsoleState::Service;
            } else {
                println!("BVAGENT DONE");
                self.state = AgentConsoleState::Done;
            }
            return None;
        };
        let line = command_wire_line(command);
        self.state = AgentConsoleState::WaitingOut { index };
        Some(line)
    }

    /// GETBEG <b64(path)> <total> <nchunks> — start reassembly.
    fn begin_get(&mut self, rest: &str) {
        let mut it = rest.split(' ');
        let path = it
            .next()
            .and_then(|b| base64_decode(b).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let total = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let nchunks = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        self.get_accum = Some(GetAccum {
            path,
            total,
            nchunks,
            bytes: Vec::with_capacity(total),
            chunks_seen: 0,
        });
    }

    /// GETCHUNK <seq> <b64(rawbytes)> — append one chunk.
    fn accum_get_chunk(&mut self, rest: &str) {
        let Some(accum) = self.get_accum.as_mut() else {
            return;
        };
        let payload = rest.split_once(' ').map_or(rest, |(_, b64)| b64);
        if let Ok(bytes) = base64_decode(payload) {
            accum.bytes.extend_from_slice(&bytes);
            accum.chunks_seen += 1;
        }
    }

    /// GETEND <seq> — finalize: verify byte count, print a summary labelled by
    /// the requesting command, and if BRIDGEVM_VIRTIO_CONSOLE_GET_DIR is set,
    /// write the file there.
    fn finish_get(&mut self, label: &str, _rest: &str) {
        let Some(accum) = self.take_finished_get() else {
            return;
        };
        let ok = accum.bytes.len() == accum.total;
        let preview_len = accum.bytes.len().min(48);
        let mut head = String::with_capacity(preview_len * 2);
        for byte in &accum.bytes[..preview_len] {
            let _ = write!(&mut head, "{byte:02x}");
        }
        let mut written_note = String::new();
        if let Ok(dir) = std::env::var("BRIDGEVM_VIRTIO_CONSOLE_GET_DIR") {
            let name = accum
                .path
                .rsplit(['\\', '/'])
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("bvagent-get.bin");
            let _ = std::fs::create_dir_all(&dir);
            let out = std::path::Path::new(&dir).join(name);
            match std::fs::write(&out, &accum.bytes) {
                Ok(()) => written_note = format!(" wrote={}", out.display()),
                Err(e) => written_note = format!(" write-error={e}"),
            }
        }
        println!(
            "BVAGENT GET {label} path={} bytes={} expected={} chunks={}/{} ok={ok}{written_note} head={head}",
            accum.path, accum.bytes.len(), accum.total, accum.chunks_seen, accum.nchunks
        );
    }

    fn take_finished_get(&mut self) -> Option<FinishedGet> {
        self.get_accum.take().map(|accum| FinishedGet {
            path: accum.path,
            total: accum.total,
            nchunks: accum.nchunks,
            bytes: accum.bytes,
            chunks_seen: accum.chunks_seen,
        })
    }

    // --- Service mode -------------------------------------------------------

    fn service_tick(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        // 30s liveness heartbeat in the log: distinguishes "service quiet, all
        // healthy" from "ticks starved" (live-diagnosed once: an idle guest
        // stopped producing vCPU exits and the whole service froze silently).
        let due = self
            .last_alive
            .is_none_or(|t| now.duration_since(t) >= Duration::from_secs(30));
        if due {
            self.last_alive = Some(now);
            println!("BVAGENT SERVICE alive t={}", self.t_ms(now));
        }
        self.service_enqueue(now);
        self.service_pump(platform, mem, now);
    }

    /// Steps 1-4 of the service loop: fold host/guest/control-file activity into
    /// the request queue. None of this touches the wire, so it needs no platform
    /// handle (and is unit-testable on its own).
    fn service_enqueue(&mut self, now: Instant) {
        // 1. Host pasteboard changes collapse to a single latest-wins ClipPush.
        //    Draining every pending change and keeping only the last means a
        //    burst of copies syncs the final value, not each intermediate one.
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            let mut latest = None;
            while let Some(text) = pasteboard.try_changed() {
                latest = Some(text);
            }
            if let Some(text) = latest {
                let normalized = normalize_clip(&text);
                if self.last_synced.as_deref() != Some(normalized.as_str()) {
                    // Oversized copies are marked seen but never pushed: the
                    // guest agent reads the wire with per-call P/Invoke cost, so
                    // a huge CLIPSET line wedges the whole channel (every later
                    // request piles into the port and times out).
                    if text.len() > self.clip_max_bytes {
                        println!(
                            "BVAGENT CLIPSYNC skip bytes={} too-large t={}",
                            text.len(),
                            self.t_ms(now)
                        );
                        self.last_synced = Some(normalized);
                    } else {
                        self.last_synced = Some(normalized);
                        self.enqueue_clip_push(text);
                    }
                }
            }
        }

        // 2. Control-file lines become Ctl commands.
        self.drain_ctl(now);

        // 3. Periodic guest clipboard poll. With clipsync on this doubles as the
        //    keepalive, so step 4 stays quiet.
        if self.clipsync {
            let due = self
                .last_clip_poll
                .is_none_or(|t| now.duration_since(t) >= self.clip_interval);
            if due && !self.any_pending(|r| matches!(r, ServiceReq::ClipPoll)) {
                self.queue.push_back(ServiceReq::ClipPoll);
            }
        }

        // 4. Heartbeat only when clipsync is off. Gate on "no ping already
        //    pending": last_send is stamped at SEND time, so a queued-but-unsent
        //    ping wouldn't move it and pings would otherwise pile up behind a
        //    long in-flight request.
        if !self.clipsync {
            let due = self
                .last_send
                .is_none_or(|t| now.duration_since(t) >= HEARTBEAT_INTERVAL);
            if due && !self.any_pending(|r| matches!(r, ServiceReq::Ping)) {
                self.queue.push_back(ServiceReq::Ping);
            }
        }

        // 5. Shared-folder sync is also lockstep: one LS/GET/PUT at a time.
        //    The engine compares guest mtimes as strings because the frozen
        //    PowerShell agent already emits a stable ISO form and the host does
        //    not need to understand Windows timestamp semantics.
        let share_due = self.share.as_ref().is_some_and(|share| {
            share
                .last_poll
                .is_none_or(|t| now.duration_since(t) >= share.interval)
        });
        if share_due && !self.share_pending() {
            if !self.enqueue_one_share_host_change() {
                self.queue.push_back(ServiceReq::ShareLs);
            }
        }
    }

    /// Timeout the in-flight request if its reply is overdue, then — lockstep —
    /// send at most one queued request.
    fn service_pump(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        if let Some((req, sent_at)) = self.in_flight.as_ref() {
            if now.duration_since(*sent_at) >= service_timeout(req) {
                println!(
                    "BVAGENT SERVICE timeout {} t={}",
                    req_kind(req),
                    now.duration_since(self.start).as_millis()
                );
                self.in_flight = None;
                // A half-received chunked GET is now orphaned; drop it so the
                // next GET starts from a clean slate.
                self.get_accum = None;
            }
        }
        if self.in_flight.is_some() {
            return;
        }
        let Some(req) = self.queue.pop_front() else {
            return;
        };
        if !self.write_service_req_line(&req, now) {
            return;
        }
        platform.virtio_console_agent_send(self.service_line_scratch.as_bytes(), mem);
        self.last_send = Some(now);
        self.in_flight = Some((req, now));
    }

    #[cfg(test)]
    fn service_req_line(&mut self, req: &ServiceReq, now: Instant) -> Option<&str> {
        self.write_service_req_line(req, now)
            .then_some(self.service_line_scratch.as_str())
    }

    fn write_service_req_line(&mut self, req: &ServiceReq, now: Instant) -> bool {
        self.service_line_scratch.clear();
        match req {
            ServiceReq::ClipPoll => {
                self.last_clip_poll = Some(now);
                self.service_line_scratch.push_str("CLIPGET\n");
                true
            }
            ServiceReq::ClipPush(text) => {
                self.service_line_scratch.push_str("CLIPSET ");
                to_guest_crlf_into(text, &mut self.clip_crlf_scratch);
                base64_encode_into(
                    self.clip_crlf_scratch.as_bytes(),
                    &mut self.service_line_scratch,
                );
                self.service_line_scratch.push('\n');
                true
            }
            ServiceReq::Ctl(command) => {
                write_command_wire_line_into(command, &mut self.service_line_scratch);
                true
            }
            ServiceReq::Ping => {
                self.service_line_scratch.push_str("PING\n");
                true
            }
            ServiceReq::ShareLs => {
                let Some(share) = self.share.as_mut() else {
                    return false;
                };
                share.last_poll = Some(now);
                self.service_line_scratch.push_str("LSR ");
                base64_encode_into(share.guest_dir.as_bytes(), &mut self.service_line_scratch);
                self.service_line_scratch.push('\n');
                true
            }
            ServiceReq::ShareGet { name } => {
                let Some(share) = self.share.as_ref() else {
                    return false;
                };
                write_share_guest_path_into(
                    &share.guest_dir,
                    name,
                    &mut self.share_guest_path_scratch,
                );
                self.service_line_scratch.push_str("GET ");
                base64_encode_into(
                    self.share_guest_path_scratch.as_bytes(),
                    &mut self.service_line_scratch,
                );
                self.service_line_scratch.push('\n');
                true
            }
            ServiceReq::SharePut {
                name, bytes, phase, ..
            } => {
                let Some(share) = self.share.as_ref() else {
                    return false;
                };
                write_share_guest_path_into(
                    &share.guest_dir,
                    name,
                    &mut self.share_guest_path_scratch,
                );
                match phase {
                    SharePutPhase::Legacy => {
                        self.service_line_scratch.push_str("PUT ");
                        base64_encode_into(
                            self.share_guest_path_scratch.as_bytes(),
                            &mut self.service_line_scratch,
                        );
                        self.service_line_scratch.push(' ');
                        base64_encode_into(bytes, &mut self.service_line_scratch);
                        self.service_line_scratch.push('\n');
                        true
                    }
                    SharePutPhase::Beg => {
                        self.service_line_scratch.push_str("PUTBEG ");
                        base64_encode_into(
                            self.share_guest_path_scratch.as_bytes(),
                            &mut self.service_line_scratch,
                        );
                        let _ = writeln!(
                            &mut self.service_line_scratch,
                            " {} {}",
                            bytes.len(),
                            bytes.len().div_ceil(SHARE_PUT_CHUNK_BYTES)
                        );
                        true
                    }
                    SharePutPhase::Chunk | SharePutPhase::End => false,
                }
            }
            ServiceReq::ShareDel { name, direction } => match direction {
                ShareDelDirection::HostToGuest => {
                    let Some(share) = self.share.as_ref() else {
                        return false;
                    };
                    write_share_guest_path_into(
                        &share.guest_dir,
                        name,
                        &mut self.share_guest_path_scratch,
                    );
                    self.service_line_scratch.push_str("DEL ");
                    base64_encode_into(
                        self.share_guest_path_scratch.as_bytes(),
                        &mut self.service_line_scratch,
                    );
                    self.service_line_scratch.push('\n');
                    true
                }
                ShareDelDirection::GuestToHost => {
                    self.handle_share_delete(name, *direction, now);
                    false
                }
            },
        }
    }

    /// Coalesce host->guest clipboard pushes: a newer host value obsoletes any
    /// still-queued (unsent) ClipPush, so drop those before queuing this one.
    /// The in-flight push is already on the wire and is left alone.
    fn enqueue_clip_push(&mut self, text: String) {
        self.queue
            .retain(|req| !matches!(req, ServiceReq::ClipPush(_)));
        self.queue.push_back(ServiceReq::ClipPush(text));
    }

    /// Whether any queued OR in-flight request satisfies `pred`.
    fn any_pending(&self, pred: impl Fn(&ServiceReq) -> bool) -> bool {
        if let Some((req, _)) = self.in_flight.as_ref() {
            if pred(req) {
                return true;
            }
        }
        self.queue.iter().any(|req| pred(req))
    }

    fn share_pending(&self) -> bool {
        self.any_pending(is_share_req)
    }

    fn enqueue_one_share_host_change(&mut self) -> bool {
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        scan_share_host_dir(share);
        for action in share
            .engine
            .on_host_scan_normalized(share.host_scan_scratch.drain(..))
        {
            match action {
                SyncAction::Get { name } => {
                    let path = share.host_dir.join(&name);
                    let Ok(bytes) = std::fs::read(&path) else {
                        continue;
                    };
                    if bytes.len() as u64 > share.engine.max_bytes() {
                        if let Some(mtime_ms) = file_mtime_ms(&path) {
                            print_host_skip_once(
                                share,
                                &name,
                                mtime_ms,
                                HostSkipKind::TooLarge,
                                bytes.len() as u64,
                            );
                        }
                        continue;
                    }
                    let Some(mtime_ms) = file_mtime_ms(&path) else {
                        continue;
                    };
                    if let Some(push) = share.engine.on_host_file(name.clone(), bytes, mtime_ms) {
                        self.queue
                            .push_back(share_put_req(name, push.bytes, push.hash));
                        return true;
                    }
                }
                SyncAction::DeleteGuest { name } => {
                    self.queue.push_back(ServiceReq::ShareDel {
                        name,
                        direction: ShareDelDirection::HostToGuest,
                    });
                    return true;
                }
                SyncAction::DeleteHost { .. } | SyncAction::Skip { .. } => {}
            }
        }
        false
    }

    fn complete_in_flight(&mut self) {
        self.in_flight = None;
    }

    /// Poll the control file (throttled) and turn appended lines into Ctl
    /// requests. A missing file means no commands yet (silent retry).
    fn drain_ctl(&mut self, now: Instant) {
        let due = self
            .ctl_last_poll
            .is_none_or(|t| now.duration_since(t) >= CTL_POLL_INTERVAL);
        if self.ctl_path.is_none() || !due {
            return;
        }
        self.ctl_last_poll = Some(now);
        self.ingest_ctl();
    }

    /// Read newly-appended control-file bytes from the saved offset and frame
    /// them into Ctl commands. A shrunk file (truncation/rewrite) resets the
    /// offset to 0 so fresh content isn't skipped. Split from the poll cadence
    /// so it is unit-testable without a clock.
    fn ingest_ctl(&mut self) {
        let Some(path) = self.ctl_path.as_deref() else {
            return;
        };
        match std::fs::metadata(path) {
            Ok(meta) if meta.len() < self.ctl_offset => {
                self.ctl_offset = 0;
                self.ctl_framer = LineFramer::new();
            }
            Ok(_) => {}
            // Missing/unreadable: no commands yet. Silent retry, no error spam.
            Err(_) => return,
        }
        if !read_ctl_appended_into(path, self.ctl_offset, &mut self.ctl_read_scratch)
            || self.ctl_read_scratch.is_empty()
        {
            return;
        }
        self.ctl_offset += self.ctl_read_scratch.len() as u64;
        self.ctl_line_scratch.clear();
        self.ctl_framer
            .push_into(&self.ctl_read_scratch, &mut self.ctl_line_scratch);
        self.ctl_read_scratch.clear();
        for line in self.ctl_line_scratch.drain(..) {
            let command = line.trim();
            if !command.is_empty() {
                self.queue.push_back(ServiceReq::Ctl(command.to_string()));
            }
        }
    }

    /// Route a guest reply in service mode by the in-flight request kind. Only
    /// the in-flight request is completed; unrelated lines are ignored (they
    /// stay in-flight until a matching reply or the stall timeout). A re-emitted
    /// READY while idle means the agent restarted, and is just noted.
    fn handle_service_reply(
        &mut self,
        line: &str,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        if matches!(
            self.in_flight.as_ref().map(|(req, _)| req),
            Some(ServiceReq::Ctl(_) | ServiceReq::ShareGet { .. })
        ) {
            if self.handle_unlabelled_get_fragment(line).is_some() {
                return;
            }
        }

        // Snapshot the kind so the &mut self helpers below don't clash with the
        // borrow on self.in_flight.
        let kind = match self.in_flight.as_ref() {
            Some((req, _)) => InFlightSnapshot::from_req(req),
            None => {
                if let Some(hostname) = line.strip_prefix("READY ") {
                    println!("BVAGENT re-READY {hostname} t={}", self.t_ms(now));
                }
                return;
            }
        };
        match kind {
            InFlightSnapshot::ClipPoll => {
                if let Some(b64) = line.strip_prefix("CLIP ") {
                    let text = base64_decode(b64)
                        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
                        .unwrap_or_default();
                    // Adopt only genuinely-new content (normalized compare) so a
                    // value we just pushed host->guest doesn't bounce straight
                    // back through the guest clipboard.
                    if let Some(normalized) = guest_clip_decision(&self.last_synced, &text) {
                        let bytes = normalized.len();
                        self.last_synced = Some(normalized.clone());
                        if let Some(pasteboard) = self.pasteboard.as_ref() {
                            pasteboard.set(normalized);
                        }
                        println!(
                            "BVAGENT CLIPSYNC guest->host bytes={bytes} t={}",
                            self.t_ms(now)
                        );
                    }
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT CLIPSYNC guest->host {line}");
                    self.complete_in_flight();
                }
            }
            InFlightSnapshot::ClipPush { guest_bytes } => {
                if line.starts_with("OK") {
                    println!(
                        "BVAGENT CLIPSYNC host->guest bytes={} t={}",
                        guest_bytes,
                        self.t_ms(now)
                    );
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT CLIPSYNC host->guest {line}");
                    self.complete_in_flight();
                }
            }
            InFlightSnapshot::Ctl(command) => match self.handle_reply_line(line, &command) {
                ReplyProgress::Complete => self.complete_in_flight(),
                ReplyProgress::Incomplete | ReplyProgress::Ignored => {}
            },
            InFlightSnapshot::Ping => {
                if line == "PONG" {
                    self.complete_in_flight();
                }
            }
            InFlightSnapshot::ShareLs => {
                if let Some(b64) = line.strip_prefix("LSOK ") {
                    match base64_decode(b64) {
                        Ok(bytes) => {
                            let listing = String::from_utf8_lossy(&bytes);
                            let actions = self
                                .share
                                .as_mut()
                                .map(|share| {
                                    share_sync::parse_ls_into(
                                        &listing,
                                        &mut share.guest_ls_scratch,
                                    );
                                    share.engine.on_guest_listing_normalized(
                                        share.guest_ls_scratch.drain(..),
                                    )
                                })
                                .unwrap_or_default();
                            for action in actions {
                                match action {
                                    SyncAction::Get { name } => {
                                        self.queue.push_back(ServiceReq::ShareGet { name });
                                    }
                                    SyncAction::DeleteHost { name } => {
                                        self.queue.push_back(ServiceReq::ShareDel {
                                            name,
                                            direction: ShareDelDirection::GuestToHost,
                                        });
                                    }
                                    SyncAction::DeleteGuest { name } => {
                                        self.queue.push_back(ServiceReq::ShareDel {
                                            name,
                                            direction: ShareDelDirection::HostToGuest,
                                        });
                                    }
                                    SyncAction::Skip { name, reason } => {
                                        print_guest_skip(&name, reason);
                                    }
                                }
                            }
                        }
                        Err(error) => {
                            println!("BVAGENT SHARE ls-error <base64 decode error: {error:?}>");
                        }
                    }
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT SHARE ls-error {line}");
                    self.complete_in_flight();
                } else if parse_out_line(line).is_some_and(|(exit, _)| exit == 255) {
                    if !self.share_old_agent_warned {
                        println!("BVAGENT SHARE agent-too-old (needs v3-share2); share disabled");
                        self.share_old_agent_warned = true;
                    }
                    self.share = None;
                    self.queue.retain(|req| !is_share_req(req));
                    self.complete_in_flight();
                }
            }
            InFlightSnapshot::ShareGet { name } => {
                if let Some(rest) = line.strip_prefix("GETEND ") {
                    self.handle_share_get_end(&name, rest, now);
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT SHARE get-error {name} {line}");
                    self.complete_in_flight();
                }
            }
            InFlightSnapshot::SharePut => {
                self.handle_share_put_reply(line, platform, mem, now);
            }
            InFlightSnapshot::ShareDel { name, direction } => {
                if line.starts_with("OK DEL") {
                    if direction == ShareDelDirection::HostToGuest {
                        if let Some(share) = self.share.as_mut() {
                            share.engine.on_guest_deleted(&name);
                        }
                        println!("BVAGENT SHARE del host->guest {name} t={}", self.t_ms(now));
                    }
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT SHARE del-error {name} {line}");
                    self.complete_in_flight();
                }
            }
        }
    }

    fn handle_share_put_reply(
        &mut self,
        line: &str,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        if line == "OK PUTBEG" {
            self.send_next_share_put_chunk(platform, mem, now);
            return;
        }

        if let Some(seq) = line
            .strip_prefix("OK PUTCHUNK ")
            .and_then(|s| s.parse::<usize>().ok())
        {
            self.advance_share_put_after_chunk(seq, platform, mem, now);
            return;
        }

        if let Some(rest) = line.strip_prefix("PUTOK ") {
            let Some((
                ServiceReq::SharePut {
                    name, bytes, hash, ..
                },
                _,
            )) = self.in_flight.take()
            else {
                return;
            };
            let len = bytes.len();
            let written = rest
                .split_once(' ')
                .and_then(|(_, n)| n.parse::<u64>().ok())
                .unwrap_or(len as u64);
            println!(
                "BVAGENT SHARE host->guest {name} bytes={written} t={}",
                self.t_ms(now)
            );
            if let Some(share) = self.share.as_mut() {
                share.engine.on_put_ok(name, len as u64, hash);
            }
            return;
        }

        if line.starts_with("ERR") {
            let Some((ServiceReq::SharePut { name, .. }, _)) = self.in_flight.take() else {
                return;
            };
            println!("BVAGENT SHARE put-error {name} {line}");
        }
    }

    fn send_next_share_put_chunk(
        &mut self,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        let Some((seq, start, end)) = self.prepare_next_share_put_chunk_payload(now) else {
            return;
        };
        if !self.write_share_put_chunk_line(seq, start, end) {
            return;
        }
        self.last_send = Some(now);
        if let (Some(platform), Some(mem)) = (platform, mem) {
            platform.virtio_console_agent_send(self.service_line_scratch.as_bytes(), mem);
        }
    }

    fn prepare_next_share_put_chunk_payload(
        &mut self,
        now: Instant,
    ) -> Option<(usize, usize, usize)> {
        let Some((
            ServiceReq::SharePut {
                bytes,
                next_chunk,
                phase,
                ..
            },
            sent_at,
        )) = self.in_flight.as_mut()
        else {
            return None;
        };
        let seq = *next_chunk;
        let start = seq.saturating_mul(SHARE_PUT_CHUNK_BYTES);
        let end = (start + SHARE_PUT_CHUNK_BYTES).min(bytes.len());
        if start > end || start >= bytes.len() {
            return None;
        }
        *next_chunk = seq + 1;
        *phase = SharePutPhase::Chunk;
        *sent_at = now;
        Some((seq, start, end))
    }

    fn advance_share_put_after_chunk(
        &mut self,
        seq: usize,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        let Some(line) = self.prepare_share_put_line_after_chunk(seq, now) else {
            return;
        };
        if !self.write_share_put_wire_line(line) {
            return;
        }
        self.last_send = Some(now);
        if let (Some(platform), Some(mem)) = (platform, mem) {
            platform.virtio_console_agent_send(self.service_line_scratch.as_bytes(), mem);
        }
    }

    fn prepare_share_put_line_after_chunk(
        &mut self,
        seq: usize,
        now: Instant,
    ) -> Option<SharePutWireLine> {
        let Some((
            ServiceReq::SharePut {
                bytes,
                next_chunk,
                phase,
                ..
            },
            sent_at,
        )) = self.in_flight.as_mut()
        else {
            return None;
        };
        if seq + 1 != *next_chunk {
            return None;
        }
        let nchunks = bytes.len().div_ceil(SHARE_PUT_CHUNK_BYTES);
        let line = if *next_chunk < nchunks {
            let send_seq = *next_chunk;
            let start = send_seq * SHARE_PUT_CHUNK_BYTES;
            let end = (start + SHARE_PUT_CHUNK_BYTES).min(bytes.len());
            *next_chunk = send_seq + 1;
            *phase = SharePutPhase::Chunk;
            SharePutWireLine::Chunk {
                seq: send_seq,
                start,
                end,
            }
        } else {
            *phase = SharePutPhase::End;
            SharePutWireLine::End { nchunks }
        };
        *sent_at = now;
        Some(line)
    }

    fn write_share_put_wire_line(&mut self, line: SharePutWireLine) -> bool {
        match line {
            SharePutWireLine::Chunk { seq, start, end } => {
                self.write_share_put_chunk_line(seq, start, end)
            }
            SharePutWireLine::End { nchunks } => {
                self.service_line_scratch.clear();
                let _ = writeln!(&mut self.service_line_scratch, "PUTEND {nchunks}");
                true
            }
        }
    }

    fn write_share_put_chunk_line(&mut self, seq: usize, start: usize, end: usize) -> bool {
        let Some((ServiceReq::SharePut { bytes, .. }, _)) = self.in_flight.as_ref() else {
            return false;
        };
        if start > end || end > bytes.len() {
            return false;
        }
        self.service_line_scratch.clear();
        let _ = write!(&mut self.service_line_scratch, "PUTCHUNK {seq} ");
        base64_encode_into(&bytes[start..end], &mut self.service_line_scratch);
        self.service_line_scratch.push('\n');
        true
    }

    fn handle_share_delete(&mut self, name: &str, direction: ShareDelDirection, now: Instant) {
        match direction {
            ShareDelDirection::HostToGuest => {}
            ShareDelDirection::GuestToHost => {
                let Some(share) = self.share.as_mut() else {
                    return;
                };
                let path = share.host_dir.join(name);
                match std::fs::remove_file(&path) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => {
                        println!("BVAGENT SHARE del-error {name} {e}");
                        return;
                    }
                }
                share.engine.on_host_deleted(name);
                println!("BVAGENT SHARE del guest->host {name} t={}", self.t_ms(now));
            }
        }
    }

    fn handle_share_get_end(&mut self, name: &str, _rest: &str, now: Instant) {
        let Some(finished) = self.take_finished_get() else {
            return;
        };
        // Never adopt a short read: a truncated transfer would poison both the
        // host copy and the recorded hash. The next LS poll simply retries.
        if finished.bytes.len() != finished.total {
            println!(
                "BVAGENT SHARE get-short {name} got={} expected={}",
                finished.bytes.len(),
                finished.total
            );
            return;
        }
        let Some(share) = self.share.as_mut() else {
            return;
        };
        match share
            .engine
            .on_guest_file(name.to_string(), finished.bytes, None)
        {
            GuestFileOutcome::AlreadySynced => {}
            GuestFileOutcome::WriteHost(bytes) => {
                let path = share.host_dir.join(name);
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&path, &bytes) {
                    Ok(()) => {
                        if let Some(mtime_ms) = file_mtime_ms(&path) {
                            share.engine.note_host_stat(name, mtime_ms);
                        }
                        println!(
                            "BVAGENT SHARE guest->host {name} bytes={} t={}",
                            bytes.len(),
                            self.t_ms(now)
                        );
                    }
                    Err(e) => println!("BVAGENT SHARE write-error {name} {e}"),
                }
            }
        }
    }
}

/// Periodic vCPU waker for service mode. The probe's main loop blocks inside
/// hv_vcpu_run, and with the in-kernel GIC an idle desktop guest can go MINUTES
/// without a userspace exit — so host-initiated service work (CLIPGET/LS polls,
/// pasteboard pushes, ctl commands, even stdout flushing) froze until the guest
/// happened to kick a virtqueue (live-observed as 5-minute log/service stalls).
/// A steady hv_vcpus_exit heartbeat bounds tick latency the same way
/// RamfbSampleLoop's sample tick does; the fired flag lets the exit dispatcher
/// tell this benign wake apart from the watchdog's EXIT_CANCELED.
pub struct ServiceWake {
    fired: Arc<AtomicBool>,
    started: bool,
}

impl ServiceWake {
    pub fn new() -> Self {
        Self {
            fired: Arc::new(AtomicBool::new(false)),
            started: false,
        }
    }

    /// Idempotently start the ticker thread. Runs for the probe's lifetime
    /// (the vCPU handle stays valid across the reboot loop's resets).
    pub fn ensure_started(&mut self, vcpu: HvVcpuT, interval: Duration) {
        if self.started {
            return;
        }
        self.started = true;
        let fired = Arc::clone(&self.fired);
        std::thread::spawn(move || loop {
            std::thread::sleep(interval);
            fired.store(true, Ordering::SeqCst);
            let v = vcpu;
            // SAFETY: Category 8 - `v` is the live HVF vCPU handle owned by
            // the probe loop, and the pointer is valid for this synchronous
            // call that requests one vCPU to leave `hv_vcpu_run`.
            unsafe { hv_vcpus_exit(&v, 1) };
        });
    }

    pub fn canceled_by_service_wake(&self, exit_reason: u32, watchdog_fired: &AtomicBool) -> bool {
        exit_reason == EXIT_CANCELED
            && self.fired.swap(false, Ordering::SeqCst)
            && !watchdog_fired.load(Ordering::SeqCst)
    }
}

#[derive(Default)]
struct LineFramer {
    pending: Vec<u8>,
}

impl LineFramer {
    fn new() -> Self {
        Self {
            pending: Vec::new(),
        }
    }

    fn push_into(&mut self, bytes: &[u8], lines: &mut Vec<String>) {
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base64DecodeError {
    InvalidByte,
    InvalidLength,
    InvalidPadding,
}

#[cfg(test)]
fn base64_encode(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    base64_encode_into(bytes, &mut out);
    out
}

fn base64_encode_into(bytes: &[u8], out: &mut String) {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    out.reserve(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0];
        let b1 = chunk.get(1).copied().unwrap_or(0);
        let b2 = chunk.get(2).copied().unwrap_or(0);
        out.push(TABLE[(b0 >> 2) as usize] as char);
        out.push(TABLE[(((b0 & 0x03) << 4) | (b1 >> 4)) as usize] as char);
        if chunk.len() >= 2 {
            out.push(TABLE[(((b1 & 0x0f) << 2) | (b2 >> 6)) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() == 3 {
            out.push(TABLE[(b2 & 0x3f) as usize] as char);
        } else {
            out.push('=');
        }
    }
}

fn base64_decode(text: &str) -> Result<Vec<u8>, Base64DecodeError> {
    let bytes = text.as_bytes();
    if bytes.len() % 4 != 0 {
        return Err(Base64DecodeError::InvalidLength);
    }
    let mut out = Vec::with_capacity(bytes.len() / 4 * 3);
    let mut saw_padding = false;
    for chunk in bytes.chunks(4) {
        let mut vals = [0u8; 4];
        let mut padding = 0usize;
        for (i, byte) in chunk.iter().copied().enumerate() {
            match byte {
                b'A'..=b'Z' if !saw_padding => vals[i] = byte - b'A',
                b'a'..=b'z' if !saw_padding => vals[i] = byte - b'a' + 26,
                b'0'..=b'9' if !saw_padding => vals[i] = byte - b'0' + 52,
                b'+' if !saw_padding => vals[i] = 62,
                b'/' if !saw_padding => vals[i] = 63,
                b'=' => {
                    saw_padding = true;
                    padding += 1;
                    if i < 2 {
                        return Err(Base64DecodeError::InvalidPadding);
                    }
                }
                _ => return Err(Base64DecodeError::InvalidByte),
            }
        }
        if padding > 2 {
            return Err(Base64DecodeError::InvalidPadding);
        }
        if padding > 0 && chunk[3] != b'=' {
            return Err(Base64DecodeError::InvalidPadding);
        }
        out.push((vals[0] << 2) | (vals[1] >> 4));
        if padding < 2 {
            out.push((vals[1] << 4) | (vals[2] >> 2));
        }
        if padding == 0 {
            out.push((vals[2] << 6) | vals[3]);
        }
    }
    Ok(out)
}

/// Verbs the guest agent handles directly (not shell commands). These are sent
/// to the agent verbatim; everything else is wrapped as `RUN <base64>`.
fn is_raw_verb(token: &str) -> bool {
    matches!(
        token,
        "CLIPGET"
            | "CLIPSET"
            | "LS"
            | "LSR"
            | "GET"
            | "PUT"
            | "PUTBEG"
            | "PUTCHUNK"
            | "PUTEND"
            | "DEL"
            | "PING"
    )
}

/// Build the wire line for a command string using the scripted-command rule: a
/// protocol verb (CLIPGET/CLIPSET/LS/GET/PUT/PING) is sent verbatim; anything
/// else is a shell line wrapped as `RUN <base64(cmd)>`. This lets both
/// BRIDGEVM_VIRTIO_CONSOLE_CMDS and the control file drive clipboard/file verbs
/// directly, e.g. "CLIPSET <b64>" or "CLIPGET".
fn command_wire_line(command: &str) -> String {
    let mut line = String::new();
    write_command_wire_line_into(command, &mut line);
    line
}

fn write_command_wire_line_into(command: &str, out: &mut String) {
    let first = command.split_whitespace().next().unwrap_or("");
    if is_raw_verb(first) {
        out.push_str(command);
        out.push('\n');
    } else {
        out.push_str("RUN ");
        base64_encode_into(command.as_bytes(), out);
        out.push('\n');
    }
}

fn parse_out_line(line: &str) -> Option<(i32, &str)> {
    let rest = line.strip_prefix("OUT ")?;
    let (exit_code, output) = rest.split_once(' ')?;
    Some((exit_code.parse().ok()?, output))
}

/// Read control-file bytes from `offset` to EOF into caller-owned storage.
/// Returns false on any IO error, which the caller treats as "nothing new yet".
fn read_ctl_appended_into(path: &str, offset: u64, out: &mut Vec<u8>) -> bool {
    use std::io::{Read, Seek, SeekFrom};
    out.clear();
    let Ok(mut file) = std::fs::File::open(path) else {
        return false;
    };
    if file.seek(SeekFrom::Start(offset)).is_err() {
        return false;
    }
    file.read_to_end(out).is_ok()
}

/// Short label for a service request, used in the stall-timeout print.
fn req_kind(req: &ServiceReq) -> &'static str {
    match req {
        ServiceReq::ClipPoll => "clip-poll",
        ServiceReq::ClipPush(_) => "clip-push",
        ServiceReq::Ctl(_) => "ctl",
        ServiceReq::Ping => "ping",
        ServiceReq::ShareLs => "share-ls",
        ServiceReq::ShareGet { .. } => "share-get",
        ServiceReq::SharePut { .. } => "share-put",
        ServiceReq::ShareDel { .. } => "share-del",
    }
}

fn service_timeout(req: &ServiceReq) -> Duration {
    let _ = req;
    SERVICE_TIMEOUT
}

fn is_share_req(req: &ServiceReq) -> bool {
    matches!(
        req,
        ServiceReq::ShareLs
            | ServiceReq::ShareGet { .. }
            | ServiceReq::SharePut { .. }
            | ServiceReq::ShareDel { .. }
    )
}

fn init_share_from_env() -> Option<ShareState> {
    let spec = match std::env::var(SHARE_ENV) {
        Ok(value) if !value.is_empty() => value,
        _ => return None,
    };
    let Some((host, guest)) = parse_share_spec(&spec) else {
        println!("BVAGENT SHARE bad spec");
        return None;
    };
    let interval_ms = env_u64(SHARE_MS_ENV, DEFAULT_SHARE_MS).max(SHARE_MS_FLOOR);
    let max_kb = env_u64(SHARE_MAX_KB_ENV, DEFAULT_SHARE_MAX_KB);
    Some(ShareState {
        engine: ShareSync::new(max_kb),
        host_dir: PathBuf::from(host),
        guest_dir: guest,
        interval: Duration::from_millis(interval_ms),
        last_poll: None,
        host_skip_seen: HashSet::new(),
        guest_ls_scratch: Vec::new(),
        host_scan_scratch: Vec::new(),
    })
}

fn parse_share_spec(spec: &str) -> Option<(String, String)> {
    let (host, guest) = spec.split_once("::")?;
    if host.is_empty() || guest.is_empty() {
        return None;
    }
    Some((host.to_string(), guest.to_string()))
}

fn scan_share_host_dir(share: &mut ShareState) {
    share.host_scan_scratch.clear();
    let root = &share.host_dir;
    let max_bytes = share.engine.max_bytes();
    let host_skip_seen = &mut share.host_skip_seen;
    scan_share_host_dir_inner(
        root,
        root,
        max_bytes,
        host_skip_seen,
        &mut share.host_scan_scratch,
    );
}

fn scan_share_host_dir_inner(
    root: &Path,
    dir: &Path,
    max_bytes: u64,
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    files: &mut Vec<HostFile>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(meta) = std::fs::symlink_metadata(&path) else {
            continue;
        };
        if meta.file_type().is_symlink() {
            continue;
        }
        if meta.is_dir() {
            scan_share_host_dir_inner(root, &path, max_bytes, host_skip_seen, files);
            continue;
        }
        if !meta.is_file() {
            continue;
        }
        let Some(name) = host_rel_path(root, &path) else {
            continue;
        };
        let mtime_ms = file_mtime_ms_from_meta(&meta).unwrap_or(0);
        let size = meta.len();
        if size > max_bytes {
            print_host_skip_once_seen(
                host_skip_seen,
                &name,
                mtime_ms,
                HostSkipKind::TooLarge,
                size,
            );
            continue;
        }
        files.push(HostFile {
            name,
            size,
            mtime_ms,
        });
    }
}

fn host_rel_path(root: &Path, path: &Path) -> Option<String> {
    let rel = path.strip_prefix(root).ok()?;
    let mut out = String::new();
    for component in rel.components() {
        let component = component.as_os_str().to_string_lossy();
        // A literal backslash is legal in a macOS filename but is a path
        // separator in the Windows guest. Treating it as a normal byte here
        // creates two different sync keys and repeats PUT/tombstone actions.
        if component.contains('\\') {
            return None;
        }
        if !out.is_empty() {
            out.push('/');
        }
        out.push_str(&component);
    }
    Some(out)
}

fn file_mtime_ms(path: &Path) -> Option<u128> {
    std::fs::metadata(path)
        .ok()
        .and_then(|meta| file_mtime_ms_from_meta(&meta))
}

fn file_mtime_ms_from_meta(meta: &std::fs::Metadata) -> Option<u128> {
    meta.modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|d| d.as_millis())
}

fn print_host_skip_once(
    share: &mut ShareState,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    print_host_skip_once_seen(&mut share.host_skip_seen, name, mtime_ms, kind, size);
}

fn print_host_skip_once_seen(
    host_skip_seen: &mut HashSet<(String, u128, HostSkipKind)>,
    name: &str,
    mtime_ms: u128,
    kind: HostSkipKind,
    size: u64,
) {
    if !host_skip_seen.insert((name.to_string(), mtime_ms, kind)) {
        return;
    }
    match kind {
        HostSkipKind::TooLarge => println!("BVAGENT SHARE skip {name} too-large {size}"),
    }
}

fn print_guest_skip(name: &str, reason: SkipReason) {
    match reason {
        SkipReason::TooLarge { size } => {
            println!("BVAGENT SHARE skip {name} too-large {size}")
        }
    }
}

fn write_share_guest_path_into(dir: &str, name: &str, out: &mut String) {
    out.clear();
    out.reserve(dir.len() + 1 + name.len());
    out.push_str(dir);
    out.push('\\');
    share_sync::append_guest_rel_into(name, out);
}

fn share_put_req(name: String, bytes: Vec<u8>, hash: u64) -> ServiceReq {
    let phase = if bytes.len() <= SHARE_PUT_CHUNK_BYTES {
        SharePutPhase::Legacy
    } else {
        SharePutPhase::Beg
    };
    ServiceReq::SharePut {
        name,
        bytes,
        hash,
        next_chunk: 0,
        phase,
    }
}

/// Fold CRLF to LF (the host/macOS convention). `last_synced` always stores this
/// form so the same content in either line-ending compares equal and doesn't
/// trigger a redundant re-sync.
fn normalize_clip(s: &str) -> String {
    s.replace("\r\n", "\n")
}

/// Convert to the guest/Windows CRLF convention WITHOUT doubling existing CRLFs:
/// normalize to LF first, then expand every LF. (A naive \n -> \r\n over text
/// that already had \r\n would yield \r\r\n.)
#[cfg(test)]
fn to_guest_crlf(s: &str) -> String {
    let mut out = String::with_capacity(to_guest_crlf_len(s));
    to_guest_crlf_into(s, &mut out);
    out
}

fn to_guest_crlf_into(s: &str, out: &mut String) {
    out.clear();
    out.reserve(to_guest_crlf_len(s));
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' && chars.peek() == Some(&'\n') {
            out.push_str("\r\n");
            let _ = chars.next();
        } else if ch == '\n' {
            out.push_str("\r\n");
        } else {
            out.push(ch);
        }
    }
}

fn to_guest_crlf_len(s: &str) -> usize {
    let bytes = s.as_bytes();
    let mut len = 0usize;
    let mut index = 0usize;
    while index < bytes.len() {
        if bytes[index] == b'\r' && bytes.get(index + 1) == Some(&b'\n') {
            len += 2;
            index += 2;
        } else if bytes[index] == b'\n' {
            len += 2;
            index += 1;
        } else {
            len += 1;
            index += 1;
        }
    }
    len
}

/// Decide whether a guest clipboard snapshot should be adopted host-side.
/// Returns the normalized (LF) text to store/apply, or None when it is empty or
/// already equal (normalized) to what we last synced.
fn guest_clip_decision(last_synced: &Option<String>, text: &str) -> Option<String> {
    if text.is_empty() {
        return None;
    }
    let normalized = normalize_clip(text);
    if last_synced.as_deref() == Some(normalized.as_str()) {
        return None;
    }
    Some(normalized)
}

fn agent_commands_from_env() -> Vec<String> {
    let value = std::env::var(CMDS_ENV).unwrap_or_else(|_| DEFAULT_CMDS.to_string());
    value
        .split('|')
        .map(str::trim)
        .filter(|cmd| !cmd.is_empty())
        .map(str::to_string)
        .collect()
}

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.trim().parse().ok())
        .unwrap_or(default)
}

fn env_flag(name: &str) -> bool {
    let Ok(value) = std::env::var(name) else {
        return false;
    };
    let value = value.trim();
    value == "1"
        || value.eq_ignore_ascii_case("true")
        || value.eq_ignore_ascii_case("yes")
        || value.eq_ignore_ascii_case("on")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn harness() -> AgentConsoleHarness {
        AgentConsoleHarness {
            start: Instant::now(),
            timeout: Duration::from_secs(1),
            scripted_test: true,
            framer: LineFramer::new(),
            inbound_scratch: Vec::new(),
            line_scratch: Vec::new(),
            state: AgentConsoleState::WaitingOut { index: 0 },
            commands: vec!["GET Zm9v".to_string()],
            last_ping: None,
            get_accum: None,
            service: false,
            clipsync: false,
            clip_interval: Duration::from_millis(1000),
            clip_max_bytes: 8 * 1024,
            queue: VecDeque::new(),
            in_flight: None,
            last_synced: None,
            pasteboard: None,
            ctl_path: None,
            ctl_offset: 0,
            ctl_framer: LineFramer::new(),
            ctl_read_scratch: Vec::new(),
            ctl_line_scratch: Vec::new(),
            service_line_scratch: String::new(),
            clip_crlf_scratch: String::new(),
            share_guest_path_scratch: String::new(),
            ctl_last_poll: None,
            last_clip_poll: None,
            last_send: None,
            last_alive: None,
            share: None,
            share_old_agent_warned: false,
        }
    }

    #[test]
    fn desktop_ready_requires_agent_handshake() {
        let mut h = harness();
        h.state = AgentConsoleState::WaitingReady;
        assert!(!h.desktop_ready());
        h.state = AgentConsoleState::WaitingPong;
        assert!(h.desktop_ready());
        h.state = AgentConsoleState::TimedOut;
        assert!(!h.desktop_ready());
    }

    #[test]
    fn only_scripted_harness_requires_per_exit_ticks() {
        let mut h = harness();
        assert!(h.per_exit_tick_needed());

        h.scripted_test = false;
        assert!(!h.per_exit_tick_needed());
    }

    fn queue_kinds(h: &AgentConsoleHarness) -> Vec<String> {
        h.queue
            .iter()
            .map(|r| match r {
                ServiceReq::ClipPoll => "poll".to_string(),
                ServiceReq::ClipPush(t) => format!("push:{t}"),
                ServiceReq::Ctl(c) => format!("ctl:{c}"),
                ServiceReq::Ping => "ping".to_string(),
                ServiceReq::ShareLs => "share-ls".to_string(),
                ServiceReq::ShareGet { name } => format!("share-get:{name}"),
                ServiceReq::SharePut { name, bytes, .. } => {
                    format!("share-put:{name}:{}", String::from_utf8_lossy(bytes))
                }
                ServiceReq::ShareDel { name, direction } => {
                    format!("share-del:{direction:?}:{name}")
                }
            })
            .collect()
    }

    fn ctl_cmds(h: &AgentConsoleHarness) -> Vec<String> {
        h.queue
            .iter()
            .filter_map(|r| match r {
                ServiceReq::Ctl(c) => Some(c.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn service_req_lines_reuse_output_scratch() {
        let mut h = harness();
        let now = Instant::now();
        let long = ServiceReq::ClipPush("x".repeat(256));
        assert!(h.write_service_req_line(&long, now));
        assert!(h.service_line_scratch.starts_with("CLIPSET "));
        let ptr = h.service_line_scratch.as_ptr();
        let capacity = h.service_line_scratch.capacity();
        let crlf_ptr = h.clip_crlf_scratch.as_ptr();
        let crlf_capacity = h.clip_crlf_scratch.capacity();

        let short = ServiceReq::Ctl("whoami".into());
        assert!(h.write_service_req_line(&short, now));
        assert!(h.service_line_scratch.starts_with("RUN "));
        assert_eq!(h.service_line_scratch.as_ptr(), ptr);
        assert_eq!(h.service_line_scratch.capacity(), capacity);

        let shorter = ServiceReq::ClipPush("y".into());
        assert!(h.write_service_req_line(&shorter, now));
        assert_eq!(h.clip_crlf_scratch.as_ptr(), crlf_ptr);
        assert_eq!(h.clip_crlf_scratch.capacity(), crlf_capacity);
    }

    #[test]
    fn share_service_lines_reuse_guest_path_scratch() {
        let mut h = harness();
        h.share = Some(test_share_state("path-scratch"));
        let now = Instant::now();

        let long = ServiceReq::ShareGet {
            name: "long/nested/path/that/should/grow/the/buffer.txt".into(),
        };
        assert!(h.write_service_req_line(&long, now));
        assert_eq!(
            h.share_guest_path_scratch,
            "C:\\share\\long\\nested\\path\\that\\should\\grow\\the\\buffer.txt"
        );
        let path_ptr = h.share_guest_path_scratch.as_ptr();
        let path_capacity = h.share_guest_path_scratch.capacity();

        let short = ServiceReq::ShareDel {
            name: "x.txt".into(),
            direction: ShareDelDirection::HostToGuest,
        };
        assert!(h.write_service_req_line(&short, now));
        assert_eq!(h.share_guest_path_scratch, "C:\\share\\x.txt");
        assert_eq!(h.share_guest_path_scratch.as_ptr(), path_ptr);
        assert_eq!(h.share_guest_path_scratch.capacity(), path_capacity);
    }

    #[test]
    fn is_raw_verb_covers_protocol_verbs_only() {
        for v in [
            "CLIPGET", "CLIPSET", "LS", "LSR", "GET", "PUT", "PUTBEG", "PUTCHUNK", "PUTEND", "DEL",
            "PING",
        ] {
            assert!(is_raw_verb(v), "{v} should be raw");
        }
        for v in ["whoami", "PS", "RUN", "ipconfig", ""] {
            assert!(!is_raw_verb(v), "{v} should be RUN-wrapped");
        }
    }

    #[test]
    fn reassembles_get_chunks_across_lines() {
        let mut h = harness();
        let payload: Vec<u8> = (0u8..=255).cycle().take(600).collect();
        let path_b64 = base64_encode(b"C:\\Windows\\Temp\\x.bin");
        h.begin_get(&format!("{path_b64} {} 3", payload.len()));
        for (seq, chunk) in payload.chunks(256).enumerate() {
            h.accum_get_chunk(&format!("{seq} {}", base64_encode(chunk)));
        }
        let accum = h.get_accum.as_ref().expect("accum present before end");
        assert_eq!(accum.bytes, payload);
        assert_eq!(accum.chunks_seen, 3);
        assert_eq!(accum.total, payload.len());
        assert_eq!(accum.path, "C:\\Windows\\Temp\\x.bin");
    }

    #[test]
    fn empty_get_reassembles_to_zero_bytes() {
        let mut h = harness();
        h.begin_get(&format!("{} 0 0", base64_encode(b"C:\\empty.txt")));
        // no GETCHUNK lines for an empty file
        let accum = h.get_accum.as_ref().unwrap();
        assert_eq!(accum.total, 0);
        assert!(accum.bytes.is_empty());
        assert_eq!(accum.nchunks, 0);
    }

    #[test]
    fn get_accum_ignores_bad_chunk_base64_but_keeps_good_ones() {
        let mut h = harness();
        h.begin_get(&format!("{} 6 2", base64_encode(b"C:\\f")));
        h.accum_get_chunk(&format!("0 {}", base64_encode(b"foo")));
        h.accum_get_chunk("1 not-valid-b64!!!"); // bad -> ignored
        h.accum_get_chunk(&format!("2 {}", base64_encode(b"bar")));
        let accum = h.get_accum.as_ref().unwrap();
        assert_eq!(accum.bytes, b"foobar");
        assert_eq!(accum.chunks_seen, 2);
    }

    #[test]
    fn base64_known_vectors_encode_and_decode() {
        let vectors = [
            ("", ""),
            ("f", "Zg=="),
            ("fo", "Zm8="),
            ("foo", "Zm9v"),
            ("foob", "Zm9vYg=="),
            ("fooba", "Zm9vYmE="),
            ("foobar", "Zm9vYmFy"),
        ];

        for (plain, encoded) in vectors {
            assert_eq!(base64_encode(plain.as_bytes()), encoded);
            assert_eq!(base64_decode(encoded).unwrap(), plain.as_bytes());
        }
    }

    #[test]
    fn base64_round_trips_binary_payloads() {
        let payloads = [
            Vec::new(),
            vec![0],
            vec![0, 1],
            vec![0, 1, 2],
            (0u8..=255).collect::<Vec<_>>(),
        ];

        for payload in payloads {
            assert_eq!(base64_decode(&base64_encode(&payload)).unwrap(), payload);
        }
    }

    #[test]
    fn line_framer_handles_partial_crlf_and_multiple_lines() {
        let mut framer = LineFramer::new();
        let mut lines = Vec::new();

        framer.push_into(b"REA", &mut lines);
        assert!(lines.is_empty());
        framer.push_into(b"DY host\r\nPONG\nOUT", &mut lines);
        assert_eq!(lines.as_slice(), ["READY host", "PONG"]);
        lines.clear();
        framer.push_into(b" 0 Zm9v", &mut lines);
        assert!(lines.is_empty());
        framer.push_into(b"\n", &mut lines);
        assert_eq!(lines.as_slice(), ["OUT 0 Zm9v"]);
    }

    #[test]
    fn line_framer_push_into_reuses_output_vec_and_keeps_tail() {
        let mut framer = LineFramer::new();
        let mut lines = Vec::with_capacity(4);
        let initial_capacity = lines.capacity();

        framer.push_into(b"one\r\ntwo\npartial", &mut lines);
        assert_eq!(lines.as_slice(), ["one", "two"]);
        assert_eq!(lines.capacity(), initial_capacity);

        lines.clear();
        framer.push_into(b"-done\r\n", &mut lines);
        assert_eq!(lines.as_slice(), ["partial-done"]);
        assert_eq!(lines.capacity(), initial_capacity);
    }

    #[test]
    fn normalize_clip_folds_crlf_to_lf() {
        assert_eq!(normalize_clip(""), "");
        assert_eq!(normalize_clip("a\r\nb"), "a\nb");
        assert_eq!(normalize_clip("a\nb"), "a\nb");
        assert_eq!(normalize_clip("a\r\nb\n"), "a\nb\n");
        assert_eq!(normalize_clip("mix\r\nof\nlines\r\n"), "mix\nof\nlines\n");
    }

    #[test]
    fn to_guest_crlf_expands_lf_without_doubling() {
        assert_eq!(to_guest_crlf(""), "");
        assert_eq!(to_guest_crlf("a\nb"), "a\r\nb");
        // Already-CRLF input must not become \r\r\n.
        assert_eq!(to_guest_crlf("a\r\nb"), "a\r\nb");
        assert_eq!(to_guest_crlf("a\r\nb\nc"), "a\r\nb\r\nc");
        assert_eq!(to_guest_crlf("trailing\n"), "trailing\r\n");
        assert_eq!(
            to_guest_crlf("mixed\r\nand\nlf\r\n"),
            "mixed\r\nand\r\nlf\r\n"
        );
    }

    #[test]
    fn to_guest_crlf_len_matches_conversion_without_allocating_result() {
        for text in [
            "",
            "plain",
            "a\nb",
            "a\r\nb",
            "a\r\nb\nc",
            "trailing\n",
            "mixed\r\nand\nlf\r\n",
            "lone\rcarriage",
            "emoji \u{1f642}\n",
        ] {
            assert_eq!(to_guest_crlf_len(text), to_guest_crlf(text).len());
        }
    }

    #[test]
    fn guest_clip_decision_syncs_only_new_normalized_text() {
        // Empty -> no-op.
        assert_eq!(guest_clip_decision(&None, ""), None);
        // New text -> adopt the normalized (LF) form.
        assert_eq!(
            guest_clip_decision(&None, "a\r\nb"),
            Some("a\nb".to_string())
        );
        // Same content as last_synced (either line-ending) -> no-op.
        let last = Some("a\nb".to_string());
        assert_eq!(guest_clip_decision(&last, "a\r\nb"), None);
        assert_eq!(guest_clip_decision(&last, "a\nb"), None);
        // Genuinely different -> adopt.
        assert_eq!(guest_clip_decision(&last, "c"), Some("c".to_string()));
    }

    #[test]
    fn clip_push_enqueue_coalesces_and_spares_inflight() {
        let mut h = harness();
        // A push already on the wire must NOT be touched by a newer host change.
        h.in_flight = Some((ServiceReq::ClipPush("inflight".into()), Instant::now()));
        h.queue.push_back(ServiceReq::ClipPush("old".into()));
        h.queue.push_back(ServiceReq::Ctl("dir".into()));

        h.enqueue_clip_push("new".into());

        // "old" dropped, "new" appended after the untouched Ctl; latest wins.
        assert_eq!(queue_kinds(&h), vec!["ctl:dir", "push:new"]);
        match h.in_flight.as_ref() {
            Some((ServiceReq::ClipPush(t), _)) => assert_eq!(t, "inflight"),
            _ => panic!("in-flight push was disturbed"),
        }
    }

    #[test]
    fn ctl_tailer_frames_partial_appends_and_resets_on_truncation() {
        use std::io::Write;

        let path = std::env::temp_dir().join(format!("bvagent-ctl-{}.txt", std::process::id()));
        let path_str = path.to_string_lossy().into_owned();
        let mut h = harness();
        h.ctl_path = Some(path_str);

        // A line split across two appends must reassemble.
        std::fs::write(&path, b"whoami\npar").unwrap();
        h.ingest_ctl();
        assert_eq!(ctl_cmds(&h), vec!["whoami"]);

        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .unwrap();
        f.write_all(b"tial\nCLIPGET\n").unwrap();
        drop(f);
        h.ingest_ctl();
        assert_eq!(ctl_cmds(&h), vec!["whoami", "partial", "CLIPGET"]);
        let read_capacity = h.ctl_read_scratch.capacity();
        let line_capacity = h.ctl_line_scratch.capacity();
        assert!(read_capacity > 0);
        assert!(line_capacity > 0);

        h.ingest_ctl();
        assert_eq!(h.ctl_read_scratch.capacity(), read_capacity);
        assert_eq!(h.ctl_line_scratch.capacity(), line_capacity);

        // Truncation (shrink below offset) restarts from the top.
        std::fs::write(&path, b"ver\n").unwrap();
        h.ingest_ctl();
        assert_eq!(ctl_cmds(&h), vec!["whoami", "partial", "CLIPGET", "ver"]);

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn missing_ctl_file_yields_no_commands() {
        let path =
            std::env::temp_dir().join(format!("bvagent-ctl-absent-{}.txt", std::process::id()));
        let _ = std::fs::remove_file(&path);
        let mut h = harness();
        h.ctl_path = Some(path.to_string_lossy().into_owned());
        h.ingest_ctl(); // must not panic / spam
        assert!(h.queue.is_empty());
    }

    #[test]
    fn empty_commands_with_service_flag_enters_service_state() {
        let mut h = harness();
        h.commands = vec![];
        h.service = true;
        h.state = AgentConsoleState::WaitingReady;
        let line = h.next_command_line(0, Instant::now());
        assert!(line.is_none());
        assert!(matches!(h.state, AgentConsoleState::Service));
        assert!(h.last_send.is_some(), "heartbeat clock anchored at entry");
    }

    #[test]
    fn empty_commands_without_service_flag_is_done() {
        let mut h = harness();
        h.commands = vec![];
        h.service = false;
        let line = h.next_command_line(0, Instant::now());
        assert!(line.is_none());
        assert!(matches!(h.state, AgentConsoleState::Done));
    }

    #[test]
    fn clip_poll_reply_updates_last_synced_without_pasteboard() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.clipsync = true;
        h.in_flight = Some((ServiceReq::ClipPoll, Instant::now()));

        // Guest clipboard "hello\r\n" -> normalized "hello\n" adopted; no panic
        // even though pasteboard is None (the host set is simply skipped).
        let b64 = base64_encode(b"hello\r\n");
        h.handle_service_reply(&format!("CLIP {b64}"), None, None, Instant::now());

        assert_eq!(h.last_synced.as_deref(), Some("hello\n"));
        assert!(h.in_flight.is_none(), "reply completes the in-flight poll");
    }

    #[test]
    fn clip_poll_not_enqueued_without_clipsync() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.clipsync = false; // no pasteboard, clipsync off

        h.service_enqueue(Instant::now());

        assert!(
            !h.queue.iter().any(|r| matches!(r, ServiceReq::ClipPoll)),
            "ClipPoll must never be queued when clipsync is off"
        );
    }

    #[test]
    fn parse_share_spec_accepts_windows_guest_paths() {
        assert_eq!(
            parse_share_spec("a::C:\\x"),
            Some(("a".to_string(), "C:\\x".to_string()))
        );
        assert_eq!(parse_share_spec("a:C:\\x"), None);
        assert_eq!(parse_share_spec("::C:\\x"), None);
        assert_eq!(parse_share_spec("a::"), None);
    }

    #[test]
    fn share_ls_not_enqueued_while_share_request_pending() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.last_send = Some(Instant::now());
        h.share = Some(test_share_state("pending"));
        h.in_flight = Some((
            ServiceReq::ShareGet {
                name: "x.txt".into(),
            },
            Instant::now(),
        ));

        h.service_enqueue(Instant::now());

        assert!(
            !h.queue.iter().any(|r| matches!(r, ServiceReq::ShareLs)),
            "ShareLs must wait until all Share requests leave the system"
        );

        h.in_flight = None;
        h.queue.push_back(share_put_req(
            "queued.txt".into(),
            b"queued".to_vec(),
            share_sync::fnv1a64(b"queued"),
        ));
        h.service_enqueue(Instant::now());
        assert_eq!(
            h.queue
                .iter()
                .filter(|r| matches!(r, ServiceReq::ShareLs))
                .count(),
            0
        );
    }

    #[test]
    fn share_put_captures_bytes_at_enqueue_time() {
        let dir = std::env::temp_dir().join(format!(
            "bvagent-share-capture-{}-{}",
            std::process::id(),
            "file"
        ));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("x.txt");
        std::fs::write(&path, b"first").unwrap();

        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.last_send = Some(Instant::now());
        h.share = Some(ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        });

        h.service_enqueue(Instant::now());
        std::fs::write(&path, b"second").unwrap();

        let put = h
            .queue
            .iter()
            .find_map(|req| match req {
                ServiceReq::SharePut {
                    name, bytes, hash, ..
                } => Some((name.clone(), bytes.clone(), *hash)),
                _ => None,
            })
            .expect("SharePut queued");
        assert_eq!(put.0, "x.txt");
        assert_eq!(put.1, b"first");
        assert_eq!(put.2, share_sync::fnv1a64(b"first"));
        assert!(
            !h.queue.iter().any(|r| matches!(r, ServiceReq::ShareLs)),
            "ShareLs waits until the captured SharePut leaves the queue"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn recursive_host_scan_finds_nested_files_and_skips_symlinks() {
        let dir =
            std::env::temp_dir().join(format!("bvagent-share-recursive-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("sub/dir")).unwrap();
        std::fs::write(dir.join("sub/dir/x.txt"), b"x").unwrap();

        #[cfg(unix)]
        std::os::unix::fs::symlink(dir.join("sub/dir/x.txt"), dir.join("link.txt")).unwrap();

        let mut share = ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        };
        scan_share_host_dir(&mut share);
        assert_eq!(share.host_scan_scratch.len(), 1);
        assert_eq!(share.host_scan_scratch[0].name, "sub/dir/x.txt");
        let capacity = share.host_scan_scratch.capacity();
        assert!(capacity > 0);

        scan_share_host_dir(&mut share);
        assert_eq!(share.host_scan_scratch.capacity(), capacity);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn host_scan_rejects_literal_backslash_names_that_windows_would_split() {
        let dir =
            std::env::temp_dir().join(format!("bvagent-share-backslash-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a\\b.txt"), b"ambiguous").unwrap();

        let mut share = ShareState {
            engine: ShareSync::new(512),
            host_dir: dir.clone(),
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        };

        scan_share_host_dir(&mut share);
        assert!(share.host_scan_scratch.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn share_put_small_payload_uses_legacy_single_line() {
        let mut h = harness();
        h.share = Some(test_share_state("small-put"));
        let req = share_put_req(
            "sub/x.txt".into(),
            b"small".to_vec(),
            share_sync::fnv1a64(b"small"),
        );
        let line = h.service_req_line(&req, Instant::now()).unwrap();
        assert!(line.starts_with("PUT "));
        assert!(!line.starts_with("PUTBEG "));
        match req {
            ServiceReq::SharePut { phase, .. } => assert_eq!(phase, SharePutPhase::Legacy),
            _ => panic!("expected SharePut"),
        }
    }

    #[test]
    fn share_put_chunked_sequence_restamps_each_step() {
        let mut h = harness();
        h.share = Some(test_share_state("chunk-put"));
        let bytes = vec![7u8; SHARE_PUT_CHUNK_BYTES * 2 + 1];
        let hash = share_sync::fnv1a64(&bytes);
        let req = share_put_req("big.bin".into(), bytes, hash);
        let t0 = Instant::now();
        let line = h.service_req_line(&req, t0).unwrap();
        assert!(line.starts_with("PUTBEG "));
        h.in_flight = Some((req, t0));

        let t1 = t0 + Duration::from_millis(1);
        h.handle_share_put_reply("OK PUTBEG", None, None, t1);
        assert!(h.service_line_scratch.starts_with("PUTCHUNK 0 "));
        let chunk_ptr = h.service_line_scratch.as_ptr();
        let chunk_capacity = h.service_line_scratch.capacity();
        match h.in_flight.as_ref().unwrap() {
            (
                ServiceReq::SharePut {
                    next_chunk, phase, ..
                },
                sent_at,
            ) => {
                assert_eq!(*next_chunk, 1);
                assert_eq!(*phase, SharePutPhase::Chunk);
                assert_eq!(*sent_at, t1);
            }
            _ => panic!("expected SharePut"),
        }

        let t2 = t0 + Duration::from_millis(2);
        h.handle_share_put_reply("OK PUTCHUNK 0", None, None, t2);
        assert!(h.service_line_scratch.starts_with("PUTCHUNK 1 "));
        assert_eq!(h.service_line_scratch.as_ptr(), chunk_ptr);
        assert_eq!(h.service_line_scratch.capacity(), chunk_capacity);
        match h.in_flight.as_ref().unwrap() {
            (
                ServiceReq::SharePut {
                    next_chunk, phase, ..
                },
                sent_at,
            ) => {
                assert_eq!(*next_chunk, 2);
                assert_eq!(*phase, SharePutPhase::Chunk);
                assert_eq!(*sent_at, t2);
            }
            _ => panic!("expected SharePut"),
        }

        let t3 = t0 + Duration::from_millis(3);
        h.handle_share_put_reply("OK PUTCHUNK 1", None, None, t3);
        assert!(h.service_line_scratch.starts_with("PUTCHUNK 2 "));
        assert_eq!(h.service_line_scratch.as_ptr(), chunk_ptr);
        assert_eq!(h.service_line_scratch.capacity(), chunk_capacity);
        match h.in_flight.as_ref().unwrap() {
            (
                ServiceReq::SharePut {
                    next_chunk, phase, ..
                },
                sent_at,
            ) => {
                assert_eq!(*next_chunk, 3);
                assert_eq!(*phase, SharePutPhase::Chunk);
                assert_eq!(*sent_at, t3);
            }
            _ => panic!("expected SharePut"),
        }

        let t4 = t0 + Duration::from_millis(4);
        h.handle_share_put_reply("OK PUTCHUNK 2", None, None, t4);
        assert_eq!(h.service_line_scratch, "PUTEND 3\n");
        assert_eq!(h.service_line_scratch.as_ptr(), chunk_ptr);
        assert_eq!(h.service_line_scratch.capacity(), chunk_capacity);
        match h.in_flight.as_ref().unwrap() {
            (ServiceReq::SharePut { phase, .. }, sent_at) => {
                assert_eq!(*phase, SharePutPhase::End);
                assert_eq!(*sent_at, t4);
            }
            _ => panic!("expected SharePut"),
        }

        h.handle_share_put_reply(
            &format!(
                "PUTOK {} {}",
                base64_encode(b"C:\\share\\big.bin"),
                SHARE_PUT_CHUNK_BYTES * 2 + 1
            ),
            None,
            None,
            t4 + Duration::from_millis(1),
        );
        assert!(h.in_flight.is_none());
    }

    #[test]
    fn old_agent_out_255_to_lsr_disables_share_once() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.share = Some(test_share_state("old-agent"));
        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));
        h.queue.push_back(ServiceReq::ShareGet {
            name: "x.txt".into(),
        });
        h.handle_service_reply(
            &format!("OUT 255 {}", base64_encode(b"unknown token: LSR")),
            None,
            None,
            Instant::now(),
        );
        assert!(h.share.is_none());
        assert!(h.share_old_agent_warned);
        assert!(h.queue.is_empty());

        h.share = Some(test_share_state("old-agent-again"));
        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));
        h.handle_service_reply(
            &format!("OUT 255 {}", base64_encode(b"unknown token: LSR")),
            None,
            None,
            Instant::now(),
        );
        assert!(h.share.is_none());
        assert!(h.share_old_agent_warned);
    }

    #[test]
    fn share_ls_reuses_guest_listing_scratch() {
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.share = Some(test_share_state("ls-scratch"));
        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));

        h.handle_service_reply(
            &format!(
                "LSOK {}",
                base64_encode(b"a.txt|1|0|2026-01-01T00:00:00.0000000Z\n")
            ),
            None,
            None,
            Instant::now(),
        );
        let capacity = h.share.as_ref().unwrap().guest_ls_scratch.capacity();
        assert!(capacity > 0);

        h.in_flight = Some((ServiceReq::ShareLs, Instant::now()));
        h.handle_service_reply(
            &format!("LSOK {}", base64_encode(b"")),
            None,
            None,
            Instant::now(),
        );
        assert_eq!(
            h.share.as_ref().unwrap().guest_ls_scratch.capacity(),
            capacity
        );
    }

    fn test_share_state(label: &str) -> ShareState {
        let dir =
            std::env::temp_dir().join(format!("bvagent-share-{label}-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&dir);
        ShareState {
            engine: ShareSync::new(512),
            host_dir: dir,
            guest_dir: "C:\\share".into(),
            interval: Duration::from_millis(500),
            last_poll: None,
            host_skip_seen: HashSet::new(),
            guest_ls_scratch: Vec::new(),
            host_scan_scratch: Vec::new(),
        }
    }
}
