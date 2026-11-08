//! A re-emitted READY is the agent-restart resynchronization point.

use super::*;

fn reapplied_after_restart(req: &ServiceReq) -> bool {
    super::dns_flush::is_dns_flush(req)
        || matches!(req, ServiceReq::Ctl(c) if super::desktop_switch::is_desktop_switch_command(c)
            || super::desktop_scale::is_desktop_scale_command(c))
}

impl AgentConsoleHarness {
    pub(super) fn resync_after_agent_restart(&mut self, hostname: &str, now: Instant) {
        println!("BVAGENT re-READY {hostname} t={}", self.t_ms(now));
        // A fresh guest agent cannot complete the previous process's
        // request. READY is therefore the only honest resynchronization
        // point at which the old wire slot may be released.
        match self.in_flight.take() {
            // An unconfirmed push may not have landed, so the guest's text is
            // re-learned.
            Some((push @ ServiceReq::ClipPush(..), _)) => {
                self.last_synced.rebaseline = true;
                let is_push = |req: &ServiceReq| matches!(req, ServiceReq::ClipPush(..));
                if !self.queue.iter().any(is_push) {
                    self.queue.push_front(push);
                }
            }
            // The old agent may have truncated the file before it stopped.
            // Resend it, so that copy ends as a write or a refusal and is
            // never taken for a guest edit.
            Some((put @ ServiceReq::SharePut { .. }, _)) => {
                self.queue.push_front(rewind_share_put_for_retransmit(put));
            }
            // Like a text push: re-learn the guest, resend unless a newer copy waits.
            Some((push @ ServiceReq::ClipImagePush(_), _)) => {
                self.last_synced.rebaseline = true;
                if !self.clip_push_pending() {
                    self.queue.push_front(rewind_clip_image_push(push));
                }
            }
            // The next CLIPX announces the image again; fetch it then.
            Some((ServiceReq::ClipImagePull { .. }, _)) => self.last_synced.image = None,
            Some((ServiceReq::TimeSync, _)) => self.requeue_time_resync(),
            // A desktop mode, a scale or a DNS flush is safe to apply twice,
            // and a lost one leaves the guest wrong until the next change.
            Some((req @ ServiceReq::Ctl(_), _)) if reapplied_after_restart(&req) => {
                let same = |queued: &ServiceReq| {
                    matches!((queued, &req), (ServiceReq::Ctl(a), ServiceReq::Ctl(b)) if a == b)
                };
                if !self.queue.iter().any(same) {
                    self.queue.push_front(req);
                }
            }
            Some((ServiceReq::AgentUpdate(step), _)) => self.agent_update_interrupted(step, now),
            _ => {}
        }
        // A guest file copy starts over: the next CLIPXF announces it again.
        if self.clip_files_stage.pull.is_some() {
            self.abandon_files_pull();
            self.last_synced.files = None;
        }
        // The new agent holds no open drop file; end the job instead of guessing its state.
        self.fail_drop("agent-restarted", now);
        self.get_accum = None;
        self.out_accum = None;
        self.note_agent_ready(hostname, now);
        // The new agent answers CLIPCAPS before any other request, and sends no
        // change notices until it has.
        self.clip_push = false;
        if self.clipsync {
            self.queue.retain(|req| !matches!(req, ServiceReq::ClipCaps));
            self.queue.push_front(ServiceReq::ClipCaps);
        }
    }
}
