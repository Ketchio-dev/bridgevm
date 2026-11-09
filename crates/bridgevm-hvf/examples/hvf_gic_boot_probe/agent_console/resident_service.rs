//! Resident clipboard, control, and shared-folder service scheduling.

use super::*;

fn is_scale(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::Ctl(c) if super::desktop_scale::is_desktop_scale_command(c))
}

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
        // 0. A requested desktop mode switch, then the display scale, a clock
        //    resync after the host slept, and a DNS cache flush after the
        //    Mac's resolver changed.
        self.enqueue_desktop_switch();
        self.enqueue_desktop_scale();
        self.enqueue_time_resync();
        self.enqueue_dns_flush();

        // 1. Host pasteboard changes collapse to a single latest-wins ClipPush.
        //    Draining every pending change and keeping only the last means a
        //    burst of copies syncs the final value, not each intermediate one.
        self.enqueue_host_clip_change(now);

        // 2. Control-file lines become Ctl commands (a BVDROP line becomes a drop job), and
        //    an active drop queues its next step.
        self.drain_ctl(now);
        self.enqueue_file_drop(now);

        // 3. Periodic guest clipboard poll. With clipsync on this doubles as the
        //    keepalive, so step 4 stays quiet. An agent that sends CLIPCHANGED
        //    is polled on each notice, and otherwise as often as the heartbeat.
        if self.clipsync {
            let interval = if self.clip_push {
                HEARTBEAT_INTERVAL
            } else {
                self.clip_interval
            };
            let due = self
                .last_clip_poll
                .is_none_or(|t| now.duration_since(t) >= interval);
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
        // A large copy's writer answers and its next range (share_big.rs).
        self.advance_big_share(now);
        let share_due = self.share.as_ref().is_some_and(|share| {
            share
                .last_poll
                .is_none_or(|t| now.duration_since(t) >= share.interval)
        });
        if share_due && !self.share_pending() && !self.enqueue_one_share_host_change(now) {
            self.queue.push_back(ServiceReq::ShareLs);
        }

        // 6. A guest tools update, after the boot traffic and before any stop.
        self.enqueue_agent_update(now);
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
        // The new mode brings a new monitor, whose scale Windows keeps apart:
        // a scale still queued would run first, so it goes after the switch.
        self.queue.retain(|req| !is_scale(req));
        super::desktop_scale::reapply_desktop_scale();
    }
    /// Keep at most one pending scale, for the latest request, queued only
    /// when no desktop switch is requested, queued or running: a queued one is
    /// replaced, and one already running gets a single follow-up.
    pub(super) fn enqueue_desktop_scale(&mut self) {
        use super::desktop_scale::{has_requested_scale, take_desktop_scale_command};
        use super::desktop_switch::{has_requested_mode, is_desktop_switch_command};
        let is_switch =
            |req: &ServiceReq| matches!(req, ServiceReq::Ctl(c) if is_desktop_switch_command(c));
        if !has_requested_scale()
            || has_requested_mode()
            || self.queue.iter().any(is_switch)
            || self
                .in_flight
                .as_ref()
                .is_some_and(|(req, _)| is_switch(req) || is_scale(req))
        {
            return;
        }
        let Some(command) = take_desktop_scale_command() else {
            return;
        };
        match self.queue.iter_mut().find(|req| is_scale(req)) {
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
            // The update's Prepare and Arm are commands too: they lock and
            // clear the folders, then rename and register.
            let cmd_in_flight = matches!(
                self.in_flight.as_ref(),
                Some((
                    ServiceReq::Ctl(_)
                        | ServiceReq::AgentUpdate(UpdateStep::Prepare | UpdateStep::Arm),
                    _
                ))
            );
            if !cmd_in_flight {
                self.overdue_beats += 1;
                if self.overdue_beats >= 3 {
                    if let Some((req, _)) = self.in_flight.take() {
                        // A large copy's range continues one already written: the copy
                        // starts over (its first GET is resent; a later range is not).
                        if self.is_big_share_step(&req) {
                            self.abort_big_share("stalled", now);
                        }
                        // A drop step creates guest files; resending could duplicate one.
                        if matches!(req, ServiceReq::ShareRange { .. }) {
                            println!("BVAGENT SERVICE dropped share-range t={}", self.t_ms(now));
                        } else if is_drop_req(&req) {
                            self.fail_drop("stalled", now);
                        } else {
                            println!(
                                "BVAGENT SERVICE retransmit {} t={}",
                                req_kind(&req),
                                now.duration_since(self.start).as_millis()
                            );
                            let req = rewind_clip_image_push(rewind_agent_update_put(
                                rewind_share_put_for_retransmit(req),
                            ));
                            self.queue.push_front(req);
                        }
                    }
                    self.overdue_beats = 0;
                }
            }
        }
        if self.in_flight.is_some() {
            return;
        }
        self.overdue_beats = 0;
        let hold_images = self.agent_update_holds_images();
        // During a large copy only its own steps go out of the shared-folder requests.
        let hold_share = self.big_share_active();
        let Some(req) = super::service_queue::pop_next_request(&mut self.queue, |req| {
            (hold_images && super::agent_update::is_clip_image_req(req))
                || (hold_share
                    && is_share_req(req)
                    && !matches!(req, ServiceReq::ShareRange { .. }))
        }) else {
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
                let verb = if self.clip_files {
                    "CLIPXF\n"
                } else if self.clip_html {
                    "CLIPXH\n"
                } else if self.clip_images.is_some() {
                    "CLIPX\n"
                } else {
                    "CLIPGET\n"
                };
                self.service_line_scratch.push_str(verb);
                true
            }
            ServiceReq::ClipCaps => {
                self.service_line_scratch.push_str("CLIPCAPS PUSH\n");
                true
            }
            ServiceReq::ClipImagePush(_) | ServiceReq::ClipImagePull { .. } => {
                self.write_clip_image_line(req)
            }
            ServiceReq::ClipFilesPull => self.write_clip_files_line(),
            ServiceReq::ClipFilesPush(push) => self.write_clip_files_push_line(push),
            ServiceReq::ClipPush(text, html) => {
                let verb = if html.is_some() { "CLIPSETH " } else { "CLIPSET " };
                self.service_line_scratch.push_str(verb);
                to_guest_crlf_into(text, &mut self.clip_crlf_scratch);
                base64_encode_into(
                    self.clip_crlf_scratch.as_bytes(),
                    &mut self.service_line_scratch,
                );
                if let Some(html) = html {
                    self.service_line_scratch.push(' ');
                    self.service_line_scratch.push_str(html);
                }
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
            ServiceReq::ShareGet { name } if self.share_big => {
                // A file the listing shows larger than a range needs room on the Mac first.
                let listed = self.share.as_ref().and_then(|share| share.engine.listed_file(name));
                if let Some((size, _)) = listed.filter(|(size, _)| *size > SHARE_RANGE_BYTES) {
                    if self.share_space_refuses(name, size) {
                        return false;
                    }
                }
                self.write_share_range_line(name, 0)
            }
            ServiceReq::ShareRange { name, offset } => self.write_share_range_line(name, *offset),
            ServiceReq::ShareGet { name } => {
                let Some(share) = self.share.as_ref() else {
                    return false;
                };
                // A new resident GET cannot inherit an earlier request's fragments.
                self.get_accum = None;
                write_share_guest_path_into(
                    share,
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
                name,
                bytes,
                phase,
                mtime_ms,
                ..
            } => {
                let Some(share) = self.share.as_ref() else {
                    return false;
                };
                write_share_guest_path_into(
                    share,
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
                        // Agents before the modification time ignore the extra token.
                        if let Some(mtime_ms) = mtime_ms {
                            let _ = write!(&mut self.service_line_scratch, " {mtime_ms}");
                        }
                        self.service_line_scratch.push('\n');
                        true
                    }
                    SharePutPhase::Beg => {
                        let chunk = self.share_put_chunk_bytes();
                        self.service_line_scratch.push_str("PUTBEG ");
                        base64_encode_into(
                            self.share_guest_path_scratch.as_bytes(),
                            &mut self.service_line_scratch,
                        );
                        let _ = write!(
                            &mut self.service_line_scratch,
                            " {} {}",
                            bytes.len(),
                            bytes.len().div_ceil(chunk)
                        );
                        if let Some(mtime_ms) = mtime_ms {
                            let _ = write!(&mut self.service_line_scratch, " {mtime_ms}");
                        }
                        self.service_line_scratch.push('\n');
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
            ServiceReq::Drop(req) => {
                write_drop_wire_line(req, &mut self.service_line_scratch);
                true
            }
            ServiceReq::AgentUpdate(step) => self.write_agent_update_line(step),
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
        self.any_pending(is_share_req) || self.big_share_active()
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
        let mut stopping = false;
        for line in self.ctl_line_scratch.drain(..) {
            let command = line.trim();
            if let Some(request) = command.strip_prefix("BVDROP ") {
                self.drops.submit(request);
            } else if !command.is_empty()
                && super::service_queue::has_foreground_capacity(&self.queue)
            {
                stopping |= self.agent_update.as_mut().is_some_and(|update| {
                    update.note_ctl_command(command);
                    update.stop_requested
                });
                self.queue.push_back(ServiceReq::Ctl(command.to_string()));
            } else if !command.is_empty() {
                eprintln!("BVAGENT CTL rejected reason=queue_full");
            }
        }
        if stopping {
            self.stop_agent_update(Instant::now());
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
        // The guest clipboard changed (CLIPCAPS PUSH). The agent writes this only
        // between replies, before it reads our next request, so a poll already
        // queued or on the wire sees the change.
        if line == "CLIPCHANGED" {
            if self.clipsync && !self.any_pending(|req| matches!(req, ServiceReq::ClipPoll)) {
                self.queue.push_back(ServiceReq::ClipPoll);
            }
            return;
        }
        // A large shared-folder copy takes its own GETBEG/GETCHUNK/GETEND (share_big.rs).
        if self.handle_big_get_line(line, now) {
            return;
        }
        if matches!(
            self.in_flight.as_ref().map(|(req, _)| req),
            Some(
                ServiceReq::Ctl(_)
                    | ServiceReq::ShareGet { .. }
                    | ServiceReq::ClipImagePull { .. }
                    | ServiceReq::ClipFilesPull
            )
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
            InFlightSnapshot::ClipCaps => self.handle_clip_caps_reply(line, now),
            InFlightSnapshot::ClipImagePush => self.handle_clip_image_push_reply(line, now, send),
            InFlightSnapshot::ClipImagePull => self.handle_clip_image_pull_reply(line, now),
            InFlightSnapshot::ClipFilesPull => self.handle_clip_files_pull_reply(line, now),
            InFlightSnapshot::ClipFilesPush => self.handle_clip_files_push_reply(line, now, send),
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
            InFlightSnapshot::Drop => self.handle_drop_reply(line, now),
            InFlightSnapshot::AgentUpdate => self.handle_agent_update_reply_with_sender(line, now, send),
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
