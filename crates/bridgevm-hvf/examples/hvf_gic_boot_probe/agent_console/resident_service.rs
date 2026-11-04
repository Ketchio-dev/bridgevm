//! Resident clipboard, control, and shared-folder service scheduling.

use super::*;

impl AgentConsoleHarness {
    // --- Service mode -------------------------------------------------------
    pub(super) fn service_tick_with_sender(&mut self, now: Instant, send: &mut dyn FnMut(&[u8])) {
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
        self.service_pump_with_sender(now, send);
    }
    /// Steps 1-4 of the service loop: fold host/guest/control-file activity into
    /// the request queue. None of this touches the wire, so it needs no platform
    /// handle (and is unit-testable on its own).
    pub(super) fn service_enqueue(&mut self, now: Instant) {
        // 0. A requested desktop mode switch, and a clock resync after the
        //    host slept.
        self.enqueue_desktop_switch();
        self.enqueue_time_resync();

        // 1. Host pasteboard changes collapse to a single latest-wins ClipPush.
        //    Draining every pending change and keeping only the last means a
        //    burst of copies syncs the final value, not each intermediate one.
        self.enqueue_host_clip_change(now);

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
        if share_due && !self.share_pending() && !self.enqueue_one_share_host_change() {
            self.queue.push_back(ServiceReq::ShareLs);
        }
    }
    /// Keep at most one pending desktop switch, for the latest requested size:
    /// a queued one is replaced, and one already running gets a single
    /// follow-up once its reply arrives.
    pub(super) fn enqueue_desktop_switch(&mut self) {
        use super::desktop_switch::{has_requested_mode, is_desktop_switch_command};
        let is_switch =
            |req: &ServiceReq| matches!(req, ServiceReq::Ctl(c) if is_desktop_switch_command(c));
        if !has_requested_mode()
            || self
                .in_flight
                .as_ref()
                .is_some_and(|(req, _)| is_switch(req))
        {
            return;
        }
        let Some(command) = super::desktop_switch::take_desktop_switch_command() else {
            return;
        };
        match self.queue.iter_mut().find(|req| is_switch(req)) {
            Some(queued) => *queued = ServiceReq::Ctl(command),
            None => self.queue.push_back(ServiceReq::Ctl(command)),
        }
    }
    /// Report an overdue in-flight request without abandoning its wire slot,
    /// then — only when the wire is truly idle — send one queued request.
    pub(super) fn service_pump_with_sender(&mut self, now: Instant, send: &mut dyn FnMut(&[u8])) {
        if let Some(kind) = self.note_service_overdue(now) {
            println!(
                "BVAGENT SERVICE overdue {kind} awaiting-reply=true t={}",
                now.duration_since(self.start).as_millis()
            );
            // Lockstep with no retransmit wedged every long session: one
            // lost console line (agent restart, guest hiccup) held
            // in_flight forever and starved ctl/share/ping behind it.
            // After 3 overdue beats the reply is not coming; drop the wire
            // slot and re-queue the request at the front. LS/GET/PUT are
            // idempotent snapshots and ping/clip polls are reads. A Cmd is
            // EXCLUDED: the single-threaded guest agent goes silent for the
            // whole runtime of a long command (gates run for minutes), so an
            // overdue Cmd usually IS still running and a resend would run it
            // twice.
            let cmd_in_flight = matches!(self.in_flight.as_ref(), Some((ServiceReq::Ctl(_), _)));
            if !cmd_in_flight {
                self.overdue_beats += 1;
                if self.overdue_beats >= 3 {
                    if let Some((req, _)) = self.in_flight.take() {
                        println!(
                            "BVAGENT SERVICE retransmit {} t={}",
                            req_kind(&req),
                            now.duration_since(self.start).as_millis()
                        );
                        self.queue.push_front(rewind_share_put_for_retransmit(req));
                    }
                    self.overdue_beats = 0;
                }
            }
        }
        if self.in_flight.is_some() {
            return;
        }
        self.overdue_beats = 0;
        let Some(req) = super::service_queue::pop_next_request(&mut self.queue) else {
            return;
        };
        if !self.write_service_req_line(&req, now) {
            if self.share.is_some()
                && matches!(
                    &req,
                    ServiceReq::ShareDel {
                        direction: ShareDelDirection::HostToGuest,
                        ..
                    }
                )
            {
                self.queue.push_front(req);
            }
            return;
        }
        send(self.service_line_scratch.as_bytes());
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
            ServiceReq::Ctl(command) => self.write_ctl_wire_line(command),
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
                // A new resident GET cannot inherit an earlier request's fragments.
                self.get_accum = None;
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
                ShareDelDirection::HostToGuest => self.write_share_delete_wire(name),
                ShareDelDirection::GuestToHost => {
                    self.handle_share_delete(name, *direction, now);
                    false
                }
            },
            ServiceReq::TimeSync => {
                self.write_time_resync_line(now);
                true
            }
        }
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
    pub(super) fn complete_in_flight(&mut self) {
        self.in_flight = None;
    }
    /// Poll the control file (throttled) and turn appended lines into Ctl
    /// requests. A missing file means no commands yet (silent retry).
    pub(super) fn drain_ctl(&mut self, now: Instant) {
        let due = super::ctl_wake::take_ctl_changed()
            || self
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
            if !command.is_empty() && super::service_queue::has_foreground_capacity(&self.queue) {
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
    pub(super) fn handle_service_reply_with_sender(
        &mut self,
        line: &str,
        now: Instant,
        send: &mut dyn FnMut(&[u8]),
    ) {
        if let Some(hostname) = line.strip_prefix("READY ") {
            self.resync_after_agent_restart(hostname, now);
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
            InFlightSnapshot::ClipPoll => self.handle_clip_poll_reply(line, now),
            InFlightSnapshot::ClipPush { guest_bytes } => {
                self.handle_clip_push_reply(line, guest_bytes, now);
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
            InFlightSnapshot::ShareLs => self.handle_share_listing_reply(line),
            InFlightSnapshot::ShareGet { name } => self.handle_share_get_reply(&name, line, now),
            InFlightSnapshot::SharePut => {
                self.handle_share_put_reply_with_sender(line, now, send);
            }
            InFlightSnapshot::TimeSync => self.handle_time_resync_reply(line),
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
}
