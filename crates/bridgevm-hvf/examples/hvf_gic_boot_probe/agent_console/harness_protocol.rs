//! Agent console scripted protocol and command response handling.

use super::*;
#[cfg(test)]
#[path = "get_bound_tests.rs"]
mod get_bound_tests;
#[path = "host_tick.rs"]
mod host_tick;
#[path = "input_receipt_label.rs"]
mod input_receipt_label;
#[path = "output_finish.rs"]
mod output_finish;
#[cfg(test)]
#[path = "transport_test_adapter.rs"]
mod transport_test_adapter;

/// Largest guest file a GET reassembles on the host.
pub(super) const MAX_AGENT_GET_BYTES: usize = 64 * 1024 * 1024;
/// Longest guest-agent reply line kept while waiting for its newline.
pub(super) const MAX_AGENT_LINE_BYTES: usize = 32 * 1024 * 1024;

impl AgentConsoleHarness {
    /// Milliseconds since boot for the BVAGENT evidence prints, so live-run
    /// latencies are measurable straight from run.log.
    pub(super) fn t_ms(&self, now: Instant) -> u128 {
        now.duration_since(self.start).as_millis()
    }
    /// Whether the main loop should run the ServiceWake ticker: resident
    /// service mode and measurement-safe scripted tests are host-driven, so
    /// they must not depend on guest activity for their tick cadence.
    pub fn service_wake_needed(&self) -> bool {
        self.service || self.periodic_test
    }
    /// Whether this harness must retain the legacy every-exit automation tick.
    /// Agent-only BOOT_TIMER runs and measurement-safe periodic tests are driven
    /// by ServiceWake and return false.
    pub const fn per_exit_tick_needed(&self) -> bool {
        self.scripted_test && !self.periodic_test
    }
    /// A READY hello or proactive PONG is emitted by the logon agent, making
    /// it a stable desktop signal that is not invalidated by clock pixels.
    pub fn desktop_ready(&self) -> bool {
        !matches!(
            self.state,
            AgentConsoleState::WaitingReady | AgentConsoleState::TimedOut
        )
    }
    pub fn from_env(start: Instant) -> Option<Self> {
        let scripted_test = env_flag(TEST_ENV);
        let periodic_test = scripted_test && env_flag(TEST_PERIODIC_ENV);
        let boot_timer_agent = env_flag(BOOT_TIMER_AGENT_ENV);
        if !scripted_test && !boot_timer_agent {
            return None;
        }
        let service = env_flag(SERVICE_ENV);
        // Clipboard auto-sync only makes sense inside the resident service loop.
        let clipsync = service && env_flag(CLIPSYNC_ENV);
        let clip_ms = env_u64(CLIPSYNC_MS_ENV, DEFAULT_CLIPSYNC_MS).max(CLIPSYNC_MS_FLOOR);
        let pasteboard = clipsync.then(|| HostPasteboard::spawn(host_pasteboard_poll_ms(clip_ms)));
        if clipsync && std::env::var_os(host_pasteboard::TEST_PASTEBOARD_ENV).is_some() {
            println!("BVAGENT CLIPSYNC test pasteboard (not the user's clipboard)");
        }
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
            periodic_test,
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
            out_since: None,
            get_accum: None,
            out_accum: None,
            service,
            clipsync,
            clip_interval: Duration::from_millis(clip_ms),
            clip_max_bytes: (env_u64(CLIPSYNC_MAX_KB_ENV, DEFAULT_CLIPSYNC_MAX_KB).max(1) * 1024)
                as usize,
            queue: VecDeque::new(),
            in_flight: None,
            overdue_beats: 0,
            last_synced: ClipBaseline::default(),
            pasteboard,
            clip_images: None,
            clip_push: false,
            put_data: false,
            clip_html: false,
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
            clock: HostClock::from_env(),
            dns_flush: DnsFlush::default(),
            drops: FileDrops::default(),
            agent_update: service.then(AgentUpdate::from_env).flatten(),
        })
    }
    /// Frame drained agent bytes into `line_scratch`. An unterminated reply
    /// longer than `MAX_AGENT_LINE_BYTES` is discarded up to its newline.
    pub(super) fn frame_inbound_lines(&mut self) {
        self.line_scratch.clear();
        let oversized = self.framer.push_bounded_into(
            &self.inbound_scratch,
            &mut self.line_scratch,
            MAX_AGENT_LINE_BYTES,
        );
        if oversized > 0 {
            eprintln!("BVAGENT rejected reason=line_too_long count={oversized}");
        }
        self.inbound_scratch.clear();
    }

    pub(super) fn handle_line_with_sender(
        &mut self,
        line: &str,
        now: Instant,
        send: &mut dyn FnMut(&[u8]),
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
                    self.note_agent_ready(hostname, now);
                    send(b"PING\n");
                    self.state = AgentConsoleState::WaitingPong;
                } else if line == "PONG" {
                    println!(
                        "BVAGENT PONG (proactive) t={}",
                        now.duration_since(self.start).as_millis()
                    );
                    self.send_next_command_with_sender(0, now, send);
                }
            }
            // The agent restarted during the handshake (a guest tools update's
            // guard does that): it never saw our PING or command.
            AgentConsoleState::WaitingPong | AgentConsoleState::WaitingOut { .. }
                if line.starts_with("READY ") =>
            {
                let hostname = &line["READY ".len()..];
                println!("BVAGENT re-READY host={hostname} (handshake) t={}", self.t_ms(now));
                self.note_agent_ready(hostname, now);
                self.restart_handshake(now, send);
            }
            AgentConsoleState::WaitingPong => {
                if line != "PONG" {
                    return;
                }
                println!("BVAGENT PONG");
                self.send_next_command_with_sender(0, now, send);
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
                        self.send_next_command_with_sender(index + 1, now, send);
                    }
                    ReplyProgress::Incomplete | ReplyProgress::Ignored => {}
                }
            }
            AgentConsoleState::Service => {
                self.handle_service_reply_with_sender(line, now, send);
            }
            AgentConsoleState::Done | AgentConsoleState::TimedOut => {}
        }
    }
    /// Interpret one guest reply line against the command currently in-flight
    /// (labelled by `command`), printing the exact "BVAGENT ..." lines scripted
    /// mode has always produced. Shared by scripted WaitingOut replies and by
    /// service-mode Ctl replies so both render identically.
    ///
    /// Chunked GET/output replies stay incomplete until GETEND/OUTEND.
    /// A command's terminal reply is one of:
    /// OUT <exit> <b64> or OUTBEG/OUTCHUNK*/OUTEND (RUN/PS), LSOK <b64> (LS),
    /// PUTOK <b64(path)> <bytes> (PUT), CLIP <b64> (CLIPGET), PONG (PING), or OK <...> /
    ///
    /// A GET reassembly ends with the command it answered: any reply that
    /// completes the command drops one still in progress.
    pub(super) fn handle_reply_line(&mut self, line: &str, command: &str) -> ReplyProgress {
        let progress = self.reply_progress(line, command);
        if matches!(progress, ReplyProgress::Complete) {
            self.get_accum = None;
        }
        progress
    }
    fn reply_progress(&mut self, line: &str, command: &str) -> ReplyProgress {
        let command = input_receipt_label::label(command);
        if let Some(progress) = handle_verb_reply(line, command) {
            return progress;
        }
        if let Some(rest) = line.strip_prefix("OUTBEG ") {
            self.begin_out(rest);
            return ReplyProgress::Incomplete;
        }
        if let Some(rest) = line.strip_prefix("OUTCHUNK ") {
            self.accum_out_chunk(rest);
            return ReplyProgress::Incomplete;
        }
        if let Some(rest) = line.strip_prefix("OUTEND ") {
            self.finish_out(command, rest);
            return ReplyProgress::Complete;
        }
        if let Some(progress) = self.handle_unlabelled_get_fragment(line) {
            return progress;
        }
        if let Some(rest) = line.strip_prefix("GETEND ") {
            return self.end_get(command, rest);
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
            // Never the guest's text: its byte length and a SHA-256 prefix.
            println!("{}", operator_clip_line(command, b64));
            return ReplyProgress::Complete;
        }
        if line.starts_with("OK") || line.starts_with("ERR") {
            println!("BVAGENT {command} -> {line}");
            return ReplyProgress::Complete;
        }
        ReplyProgress::Ignored
    }
    pub(super) fn handle_unlabelled_get_fragment(&mut self, line: &str) -> Option<ReplyProgress> {
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
    pub(super) fn send_next_command_with_sender(
        &mut self,
        index: usize,
        now: Instant,
        send: &mut dyn FnMut(&[u8]),
    ) {
        if let Some(line) = self.next_command_line(index, now) {
            send(line.as_bytes());
        }
    }
    /// Advance to command `index`: returns the wire bytes to send and moves to
    /// WaitingOut, or None when the list is exhausted — transitioning to the
    /// resident Service state (if enabled) or Done. Split from the send so the
    /// terminal transition is unit-testable without a platform handle.
    pub(super) fn next_command_line(&mut self, index: usize, now: Instant) -> Option<String> {
        let Some(command) = self.commands.get(index) else {
            if self.service {
                println!("BVAGENT SERVICE start t={}", self.t_ms(now));
                // Desktop modes requested before the agent was up.
                self.enqueue_desktop_switch();
                // Whether this agent can move clipboard images (clip_image.rs).
                if self.clipsync {
                    self.queue.push_front(ServiceReq::ClipCaps);
                }
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
        self.out_since = Some(now);
        self.state = AgentConsoleState::WaitingOut { index };
        Some(line)
    }
    /// PING again and wait for the PONG; the scripted commands then start over.
    pub(super) fn restart_handshake(&mut self, now: Instant, send: &mut dyn FnMut(&[u8])) {
        self.get_accum = None;
        self.out_accum = None;
        send(b"PING\n");
        self.last_ping = Some(now);
        self.state = AgentConsoleState::WaitingPong;
    }
    /// GETBEG <b64(path)> <total> <nchunks> — start reassembly.
    pub(super) fn begin_get(&mut self, rest: &str) {
        let mut it = rest.split(' ');
        let path = it
            .next()
            .and_then(|b| base64_decode(b).ok())
            .map(|b| String::from_utf8_lossy(&b).into_owned())
            .unwrap_or_default();
        let total = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        let nchunks = it.next().and_then(|s| s.parse().ok()).unwrap_or(0);
        // The guest declares `total`; never reserve or accept past the bound.
        // An oversized GET accumulates nothing and ends as a short read.
        self.get_accum = Some(GetAccum {
            path,
            total,
            nchunks,
            bytes: Vec::with_capacity(if total <= MAX_AGENT_GET_BYTES {
                total
            } else {
                0
            }),
            chunks_seen: 0,
        });
    }
    /// GETCHUNK <seq> <b64(rawbytes)> — append one chunk.
    pub(super) fn accum_get_chunk(&mut self, rest: &str) {
        let Some(accum) = self.get_accum.as_mut() else {
            return;
        };
        let payload = rest.split_once(' ').map_or(rest, |(_, b64)| b64);
        if let Ok(bytes) = base64_decode(payload) {
            let limit = if accum.total <= MAX_AGENT_GET_BYTES {
                accum.total
            } else {
                0
            };
            if accum.bytes.len().saturating_add(bytes.len()) > limit {
                return;
            }
            accum.bytes.extend_from_slice(&bytes);
            accum.chunks_seen += 1;
        }
    }
    pub(super) fn take_finished_get(&mut self) -> Option<FinishedGet> {
        self.get_accum.take().map(|accum| FinishedGet {
            path: accum.path,
            total: accum.total,
            nchunks: accum.nchunks,
            bytes: accum.bytes,
            chunks_seen: accum.chunks_seen,
        })
    }
    /// OUTBEG <exit> <total> <nchunks> — start bounded command-output
    /// reassembly. Oversized declarations are remembered as invalid without
    /// reserving attacker-controlled memory; OUTEND will still release the
    /// lockstep wire slot with an explicit protocol error.
    pub(super) fn begin_out(&mut self, rest: &str) {
        let mut fields = rest.split_whitespace();
        let exit_code = fields.next().and_then(|value| value.parse::<i32>().ok());
        let total = fields.next().and_then(|value| value.parse::<usize>().ok());
        let nchunks = fields.next().and_then(|value| value.parse::<usize>().ok());
        let parsed =
            exit_code.is_some() && total.is_some() && nchunks.is_some() && fields.next().is_none();
        let total = total.unwrap_or(0);
        let expected_chunks = total.div_ceil(COMMAND_OUTPUT_CHUNK_BYTES);
        let valid = parsed && total <= MAX_COMMAND_OUTPUT_BYTES && nchunks == Some(expected_chunks);
        self.out_accum = Some(OutAccum {
            exit_code: exit_code.unwrap_or(-1),
            total,
            nchunks: nchunks.unwrap_or(0),
            bytes: Vec::with_capacity(if valid { total } else { 0 }),
            chunks_seen: 0,
            valid,
        });
    }
    /// OUTCHUNK <seq> <b64(rawbytes)> — append exactly the expected next
    /// fragment. Sequence, count, base64, and declared-size errors poison the
    /// reply rather than silently returning partial command output.
    pub(super) fn accum_out_chunk(&mut self, rest: &str) {
        let Some(accum) = self.out_accum.as_mut() else {
            return;
        };
        let Some((seq, payload)) = rest.split_once(' ') else {
            accum.valid = false;
            return;
        };
        let Ok(seq) = seq.parse::<usize>() else {
            accum.valid = false;
            return;
        };
        let expected_seq = accum.chunks_seen;
        accum.chunks_seen = accum.chunks_seen.saturating_add(1);
        // Once a declaration or prior fragment has poisoned the envelope, do
        // not decode or perform offset arithmetic on attacker-controlled
        // counts. OUTEND will still terminate the wire slot with an error.
        if !accum.valid {
            return;
        }
        if seq != expected_seq || expected_seq >= accum.nchunks {
            accum.valid = false;
            return;
        }
        if payload.len() > COMMAND_OUTPUT_CHUNK_B64_BYTES {
            accum.valid = false;
            return;
        }
        let Ok(bytes) = base64_decode(payload) else {
            accum.valid = false;
            return;
        };
        let Some(next_len) = accum.bytes.len().checked_add(bytes.len()) else {
            accum.valid = false;
            return;
        };
        let Some(chunk_offset) = expected_seq.checked_mul(COMMAND_OUTPUT_CHUNK_BYTES) else {
            accum.valid = false;
            return;
        };
        let expected_len = if expected_seq + 1 == accum.nchunks {
            let Some(remaining) = accum.total.checked_sub(chunk_offset) else {
                accum.valid = false;
                return;
            };
            remaining
        } else {
            COMMAND_OUTPUT_CHUNK_BYTES
        };
        if bytes.len() != expected_len || next_len > accum.total {
            accum.valid = false;
            return;
        }
        accum.bytes.extend_from_slice(&bytes);
    }
}

/// The pasteboard thread's cadence: a Mac copy reaches the guest, and a guest
/// copy the Mac, within about this much plus a service tick. Never slower than
/// the guest poll, which tests shorten.
pub(super) fn host_pasteboard_poll_ms(clip_ms: u64) -> u64 {
    clip_ms.min(HOST_PASTEBOARD_POLL_MS)
}

#[cfg(test)]
mod pasteboard_poll_tests {
    use super::*;

    #[test]
    fn the_mac_pasteboard_is_checked_ten_times_per_guest_poll() {
        // It used to share the guest's 1 s cadence, so a Mac copy pasted in
        // Windows within a second was still the old clipboard.
        assert_eq!(host_pasteboard_poll_ms(DEFAULT_CLIPSYNC_MS), 100);
        assert_eq!(host_pasteboard_poll_ms(CLIPSYNC_MS_FLOOR), CLIPSYNC_MS_FLOOR);
    }
}
