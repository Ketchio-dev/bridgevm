//! Resident clipboard, control, and shared-folder service scheduling.

use super::*;

impl AgentConsoleHarness {
    // --- Service mode -------------------------------------------------------
    pub(super) fn service_tick(
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
            .map_or(true, |t| now.duration_since(t) >= Duration::from_secs(30));
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
    pub(super) fn service_enqueue(&mut self, now: Instant) {
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
                .map_or(true, |t| now.duration_since(t) >= self.clip_interval);
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
                .map_or(true, |t| now.duration_since(t) >= HEARTBEAT_INTERVAL);
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
                .map_or(true, |t| now.duration_since(t) >= share.interval)
        });
        if share_due && !self.share_pending() && !self.enqueue_one_share_host_change() {
            self.queue.push_back(ServiceReq::ShareLs);
        }
    }
    /// Report an overdue in-flight request without abandoning its wire slot,
    /// then — only when the wire is truly idle — send one queued request.
    pub(super) fn service_pump(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        now: Instant,
    ) {
        if let Some(kind) = self.note_service_overdue(now) {
            println!(
                "BVAGENT SERVICE overdue {kind} awaiting-reply=true t={}",
                now.duration_since(self.start).as_millis()
            );
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
    pub(super) fn note_service_overdue(&mut self, now: Instant) -> Option<&'static str> {
        let (req, sent_at) = self.in_flight.as_mut()?;
        if now.duration_since(*sent_at) < SERVICE_OVERDUE_INTERVAL {
            return None;
        }
        // Re-anchor the reporting interval while retaining the request. This
        // is an overdue heartbeat, not permission to violate lockstep.
        *sent_at = now;
        Some(req_kind(req))
    }
    #[cfg(test)]
    pub(super) fn service_req_line(&mut self, req: &ServiceReq, now: Instant) -> Option<&str> {
        self.write_service_req_line(req, now)
            .then_some(self.service_line_scratch.as_str())
    }
    pub(super) fn write_service_req_line(&mut self, req: &ServiceReq, now: Instant) -> bool {
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
    pub(super) fn enqueue_clip_push(&mut self, text: String) {
        self.queue
            .retain(|req| !matches!(req, ServiceReq::ClipPush(_)));
        self.queue.push_back(ServiceReq::ClipPush(text));
    }
    /// Whether any queued OR in-flight request satisfies `pred`.
    pub(super) fn any_pending(&self, pred: impl Fn(&ServiceReq) -> bool) -> bool {
        if let Some((req, _)) = self.in_flight.as_ref() {
            if pred(req) {
                return true;
            }
        }
        self.queue.iter().any(pred)
    }
    pub(super) fn share_pending(&self) -> bool {
        self.any_pending(is_share_req)
    }
    pub(super) fn enqueue_one_share_host_change(&mut self) -> bool {
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
                    let bytes = match read_share_file_bounded(&path, share.engine.max_bytes()) {
                        Ok(bytes) => bytes,
                        Err(ShareFileReadError::TooLarge(size)) => {
                            if let Some(mtime_ms) = file_mtime_ms(&path) {
                                print_host_skip_once(
                                    share,
                                    &name,
                                    mtime_ms,
                                    HostSkipKind::TooLarge,
                                    size,
                                );
                            }
                            continue;
                        }
                        Err(ShareFileReadError::Io) => continue,
                    };
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
    pub(super) fn complete_in_flight(&mut self) {
        self.in_flight = None;
    }
    /// Poll the control file (throttled) and turn appended lines into Ctl
    /// requests. A missing file means no commands yet (silent retry).
    pub(super) fn drain_ctl(&mut self, now: Instant) {
        let due = self
            .ctl_last_poll
            .map_or(true, |t| now.duration_since(t) >= CTL_POLL_INTERVAL);
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
    pub(super) fn ingest_ctl(&mut self) {
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
        let oversized = self.ctl_framer.push_bounded_into(
            &self.ctl_read_scratch,
            &mut self.ctl_line_scratch,
            MAX_CTL_COMMAND_BYTES,
        );
        if oversized > 0 {
            eprintln!("BVAGENT CTL rejected reason=command_too_long count={oversized}");
        }
        self.ctl_read_scratch.clear();
        for line in self.ctl_line_scratch.drain(..) {
            let command = line.trim();
            if !command.is_empty() && self.queue.len() < MAX_SERVICE_QUEUE_DEPTH {
                self.queue.push_back(ServiceReq::Ctl(command.to_string()));
            } else if !command.is_empty() {
                eprintln!("BVAGENT CTL rejected reason=queue_full");
            }
        }
    }
    /// Route a guest reply in service mode by the in-flight request kind. Only
    /// the in-flight request is completed; unrelated lines are ignored and the
    /// request stays in-flight until its matching reply. A re-emitted READY is
    /// the explicit agent-restart resynchronization point.
    pub(super) fn handle_service_reply(
        &mut self,
        line: &str,
        platform: Option<&mut VirtPlatform>,
        mem: Option<&mut dyn GuestMemoryMut>,
        now: Instant,
    ) {
        if let Some(hostname) = line.strip_prefix("READY ") {
            println!("BVAGENT re-READY {hostname} t={}", self.t_ms(now));
            // A fresh guest agent cannot complete the previous process's
            // request. READY is therefore the only honest resynchronization
            // point at which the old wire slot may be released.
            self.in_flight = None;
            self.get_accum = None;
            self.out_accum = None;
            return;
        }
        if matches!(
            self.in_flight.as_ref().map(|(req, _)| req),
            Some(ServiceReq::Ctl(_) | ServiceReq::ShareGet { .. })
        ) && self.handle_unlabelled_get_fragment(line).is_some()
        {
            return;
        }

        // Snapshot the kind so the &mut self helpers below don't clash with the
        // borrow on self.in_flight.
        let kind = match self.in_flight.as_ref() {
            Some((req, _)) => InFlightSnapshot::from_req(req),
            None => return,
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
    pub(super) fn handle_share_put_reply(
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
    pub(super) fn send_next_share_put_chunk(
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
    pub(super) fn prepare_next_share_put_chunk_payload(
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
    pub(super) fn advance_share_put_after_chunk(
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
    pub(super) fn prepare_share_put_line_after_chunk(
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
    pub(super) fn write_share_put_wire_line(&mut self, line: SharePutWireLine) -> bool {
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
    pub(super) fn write_share_put_chunk_line(
        &mut self,
        seq: usize,
        start: usize,
        end: usize,
    ) -> bool {
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
    pub(super) fn handle_share_delete(
        &mut self,
        name: &str,
        direction: ShareDelDirection,
        now: Instant,
    ) {
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
    pub(super) fn handle_share_get_end(&mut self, name: &str, _rest: &str, now: Instant) {
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
