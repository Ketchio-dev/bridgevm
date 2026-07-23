//! Agent console scripted protocol and command response handling.

use super::*;

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
    /// it a stable desktop oracle that is not invalidated by clock pixels.
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
            get_accum: None,
            out_accum: None,
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
                .map_or(true, |t| now.duration_since(t) >= PING_INTERVAL);
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
    pub(super) fn handle_line(
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
    /// Chunked GET and command-output replies span several lines and stay
    /// Incomplete until GETEND/OUTEND. A command's terminal reply is one of:
    /// OUT <exit> <b64> or OUTBEG/OUTCHUNK*/OUTEND (RUN/PS), LSOK <b64> (LS),
    /// PUTOK <b64(path)> <bytes> (PUT), CLIP <b64> (CLIPGET), or OK <...> /
    /// ERR <...> (CLIPSET & other verbs). Anything else is stray.
    pub(super) fn handle_reply_line(&mut self, line: &str, command: &str) -> ReplyProgress {
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
    pub(super) fn send_next_command_or_done(
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
    pub(super) fn next_command_line(&mut self, index: usize, now: Instant) -> Option<String> {
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
    pub(super) fn begin_get(&mut self, rest: &str) {
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
    pub(super) fn accum_get_chunk(&mut self, rest: &str) {
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
    pub(super) fn finish_get(&mut self, label: &str, _rest: &str) {
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
    /// OUTEND <nchunks> — verify the full frame and render the historical
    /// BVAGENT CMD/END envelope so existing log consumers remain compatible.
    pub(super) fn finish_out(&mut self, command: &str, rest: &str) {
        let Some(accum) = self.out_accum.take() else {
            println!(
                "BVAGENT CMD {command} exit=-1\n<chunked output protocol error: OUTEND without OUTBEG>\nBVAGENT END {command}"
            );
            return;
        };
        let end_count = rest.trim().parse::<usize>().ok();
        let valid = accum.valid
            && end_count == Some(accum.nchunks)
            && accum.chunks_seen == accum.nchunks
            && accum.bytes.len() == accum.total;
        if valid {
            let text = String::from_utf8_lossy(&accum.bytes);
            println!(
                "BVAGENT CMD {command} exit={}\n{text}\nBVAGENT END {command}",
                accum.exit_code
            );
        } else {
            println!(
                "BVAGENT CMD {command} exit=-1\n<chunked output protocol error: declared-exit={} bytes={}/{} chunks={}/{} end={}>\nBVAGENT END {command}",
                accum.exit_code,
                accum.bytes.len(),
                accum.total,
                accum.chunks_seen,
                accum.nchunks,
                end_count
                    .map(|value| value.to_string())
                    .unwrap_or_else(|| "invalid".to_string())
            );
        }
    }
}
