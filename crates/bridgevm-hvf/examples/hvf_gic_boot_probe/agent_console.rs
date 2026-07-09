use std::collections::VecDeque;
use std::time::{Duration, Instant};

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;

#[path = "host_pasteboard.rs"]
mod host_pasteboard;

use host_pasteboard::HostPasteboard;

const TEST_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST";
const CMDS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CMDS";
const TIMEOUT_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_TEST_TIMEOUT_MS";
const SERVICE_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_SERVICE";
const CLIPSYNC_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC";
const CLIPSYNC_MS_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC_MS";
const CTL_ENV: &str = "BRIDGEVM_VIRTIO_CONSOLE_CTL";
const DEFAULT_CMDS: &str = "whoami|ver|ipconfig";
const DEFAULT_TIMEOUT_MS: u64 = 180_000;
const DEFAULT_CLIPSYNC_MS: u64 = 1000;
const CLIPSYNC_MS_FLOOR: u64 = 100;

pub struct AgentConsoleHarness {
    start: Instant,
    timeout: Duration,
    framer: LineFramer,
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
    /// Throttle for control-file stat/reads.
    ctl_last_poll: Option<Instant>,
    /// Last time a CLIPGET was sent (clip poll cadence).
    last_clip_poll: Option<Instant>,
    /// Last time anything was sent to the guest (heartbeat cadence).
    last_send: Option<Instant>,
}

/// Reassembly state for a chunked GET reply (GETBEG -> GETCHUNK* -> GETEND).
struct GetAccum {
    path: String,
    total: usize,
    nchunks: usize,
    bytes: Vec<u8>,
    chunks_seen: usize,
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

impl AgentConsoleHarness {
    pub fn from_env(start: Instant) -> Option<Self> {
        if !env_flag(TEST_ENV) {
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
        Some(Self {
            start,
            timeout: Duration::from_millis(env_u64(TIMEOUT_MS_ENV, DEFAULT_TIMEOUT_MS)),
            framer: LineFramer::new(),
            state: AgentConsoleState::WaitingReady,
            commands: agent_commands_from_env(),
            last_ping: None,
            get_accum: None,
            service,
            clipsync,
            clip_interval: Duration::from_millis(clip_ms),
            queue: VecDeque::new(),
            in_flight: None,
            last_synced: None,
            pasteboard,
            ctl_path,
            ctl_offset: 0,
            ctl_framer: LineFramer::new(),
            ctl_last_poll: None,
            last_clip_poll: None,
            last_send: None,
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

        let inbound = platform.virtio_console_agent_take_inbound();
        let lines = self.framer.push(&inbound);
        for line in lines {
            self.handle_line(&line, platform, mem, now);
        }

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
                let command = self.commands[index].clone();
                match self.handle_reply_line(line, &command) {
                    ReplyProgress::Complete => {
                        self.send_next_command_or_done(index + 1, platform, mem, now);
                    }
                    ReplyProgress::Incomplete | ReplyProgress::Ignored => {}
                }
            }
            AgentConsoleState::Service => {
                self.handle_service_reply(line);
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
        if let Some(rest) = line.strip_prefix("GETBEG ") {
            self.begin_get(rest);
            return ReplyProgress::Incomplete;
        }
        if let Some(rest) = line.strip_prefix("GETCHUNK ") {
            self.accum_get_chunk(rest);
            return ReplyProgress::Incomplete;
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
                println!("BVAGENT SERVICE start");
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
        let Some(accum) = self.get_accum.take() else {
            return;
        };
        let ok = accum.bytes.len() == accum.total;
        let head: String = accum
            .bytes
            .iter()
            .take(48)
            .map(|b| format!("{b:02x}"))
            .collect();
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

    // --- Service mode -------------------------------------------------------

    fn service_tick(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
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
                    self.last_synced = Some(normalized);
                    self.enqueue_clip_push(text);
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
            if now.duration_since(*sent_at) >= SERVICE_TIMEOUT {
                println!("BVAGENT SERVICE timeout {}", req_kind(req));
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
        let line = match &req {
            ServiceReq::ClipPoll => {
                self.last_clip_poll = Some(now);
                "CLIPGET\n".to_string()
            }
            ServiceReq::ClipPush(text) => {
                format!(
                    "CLIPSET {}\n",
                    base64_encode(to_guest_crlf(text).as_bytes())
                )
            }
            ServiceReq::Ctl(command) => command_wire_line(command),
            ServiceReq::Ping => "PING\n".to_string(),
        };
        platform.virtio_console_agent_send(line.as_bytes(), mem);
        self.last_send = Some(now);
        self.in_flight = Some((req, now));
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
        let Some(path) = self.ctl_path.clone() else {
            return;
        };
        match std::fs::metadata(&path) {
            Ok(meta) if meta.len() < self.ctl_offset => {
                self.ctl_offset = 0;
                self.ctl_framer = LineFramer::new();
            }
            Ok(_) => {}
            // Missing/unreadable: no commands yet. Silent retry, no error spam.
            Err(_) => return,
        }
        let appended = match read_ctl_appended(&path, self.ctl_offset) {
            Some(bytes) if !bytes.is_empty() => bytes,
            _ => return,
        };
        self.ctl_offset += appended.len() as u64;
        for line in self.ctl_framer.push(&appended) {
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
    fn handle_service_reply(&mut self, line: &str) {
        // Snapshot the kind so the &mut self helpers below don't clash with the
        // borrow on self.in_flight.
        let kind = match self.in_flight.as_ref() {
            Some((req, _)) => req.clone(),
            None => {
                if let Some(hostname) = line.strip_prefix("READY ") {
                    println!("BVAGENT re-READY {hostname}");
                }
                return;
            }
        };
        match kind {
            ServiceReq::ClipPoll => {
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
                        println!("BVAGENT CLIPSYNC guest->host bytes={bytes}");
                    }
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT CLIPSYNC guest->host {line}");
                    self.complete_in_flight();
                }
            }
            ServiceReq::ClipPush(text) => {
                if line.starts_with("OK") {
                    println!(
                        "BVAGENT CLIPSYNC host->guest bytes={}",
                        to_guest_crlf(&text).len()
                    );
                    self.complete_in_flight();
                } else if line.starts_with("ERR") {
                    println!("BVAGENT CLIPSYNC host->guest {line}");
                    self.complete_in_flight();
                }
            }
            ServiceReq::Ctl(command) => match self.handle_reply_line(line, &command) {
                ReplyProgress::Complete => self.complete_in_flight(),
                ReplyProgress::Incomplete | ReplyProgress::Ignored => {}
            },
            ServiceReq::Ping => {
                if line == "PONG" {
                    self.complete_in_flight();
                }
            }
        }
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

    fn push(&mut self, bytes: &[u8]) -> Vec<String> {
        self.pending.extend_from_slice(bytes);
        let mut lines = Vec::new();
        while let Some(newline) = self.pending.iter().position(|byte| *byte == b'\n') {
            let mut raw = self.pending.drain(..=newline).collect::<Vec<_>>();
            raw.pop();
            if raw.last() == Some(&b'\r') {
                raw.pop();
            }
            lines.push(String::from_utf8_lossy(&raw).into_owned());
        }
        lines
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Base64DecodeError {
    InvalidByte,
    InvalidLength,
    InvalidPadding,
}

fn base64_encode(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
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
    out
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
    matches!(token, "CLIPGET" | "CLIPSET" | "LS" | "GET" | "PUT" | "PING")
}

/// Build the wire line for a command string using the scripted-command rule: a
/// protocol verb (CLIPGET/CLIPSET/LS/GET/PUT/PING) is sent verbatim; anything
/// else is a shell line wrapped as `RUN <base64(cmd)>`. This lets both
/// BRIDGEVM_VIRTIO_CONSOLE_CMDS and the control file drive clipboard/file verbs
/// directly, e.g. "CLIPSET <b64>" or "CLIPGET".
fn command_wire_line(command: &str) -> String {
    let first = command.split_whitespace().next().unwrap_or("");
    if is_raw_verb(first) {
        format!("{command}\n")
    } else {
        format!("RUN {}\n", base64_encode(command.as_bytes()))
    }
}

fn parse_out_line(line: &str) -> Option<(i32, &str)> {
    let rest = line.strip_prefix("OUT ")?;
    let (exit_code, output) = rest.split_once(' ')?;
    Some((exit_code.parse().ok()?, output))
}

/// Read control-file bytes from `offset` to EOF. None on any IO error, which the
/// caller treats as "nothing new yet".
fn read_ctl_appended(path: &str, offset: u64) -> Option<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let mut buf = Vec::new();
    file.read_to_end(&mut buf).ok()?;
    Some(buf)
}

/// Short label for a service request, used in the stall-timeout print.
fn req_kind(req: &ServiceReq) -> &'static str {
    match req {
        ServiceReq::ClipPoll => "clip-poll",
        ServiceReq::ClipPush(_) => "clip-push",
        ServiceReq::Ctl(_) => "ctl",
        ServiceReq::Ping => "ping",
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
fn to_guest_crlf(s: &str) -> String {
    normalize_clip(s).replace('\n', "\r\n")
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
            framer: LineFramer::new(),
            state: AgentConsoleState::WaitingOut { index: 0 },
            commands: vec!["GET Zm9v".to_string()],
            last_ping: None,
            get_accum: None,
            service: false,
            clipsync: false,
            clip_interval: Duration::from_millis(1000),
            queue: VecDeque::new(),
            in_flight: None,
            last_synced: None,
            pasteboard: None,
            ctl_path: None,
            ctl_offset: 0,
            ctl_framer: LineFramer::new(),
            ctl_last_poll: None,
            last_clip_poll: None,
            last_send: None,
        }
    }

    fn queue_kinds(h: &AgentConsoleHarness) -> Vec<String> {
        h.queue
            .iter()
            .map(|r| match r {
                ServiceReq::ClipPoll => "poll".to_string(),
                ServiceReq::ClipPush(t) => format!("push:{t}"),
                ServiceReq::Ctl(c) => format!("ctl:{c}"),
                ServiceReq::Ping => "ping".to_string(),
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
    fn is_raw_verb_covers_protocol_verbs_only() {
        for v in ["CLIPGET", "CLIPSET", "LS", "GET", "PUT", "PING"] {
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

        assert!(framer.push(b"REA").is_empty());
        assert_eq!(
            framer.push(b"DY host\r\nPONG\nOUT"),
            vec!["READY host", "PONG"]
        );
        assert!(framer.push(b" 0 Zm9v").is_empty());
        assert_eq!(framer.push(b"\n").as_slice(), ["OUT 0 Zm9v"]);
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
        h.handle_service_reply(&format!("CLIP {b64}"));

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
}
