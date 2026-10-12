//! A re-emitted READY is the agent-restart resynchronization point.

use super::*;

impl AgentConsoleHarness {
    pub(super) fn resync_after_agent_restart(&mut self, hostname: &str, now: Instant) {
        println!("BVAGENT re-READY {hostname} t={}", self.t_ms(now));
        // A fresh guest agent cannot complete the previous process's
        // request. READY is therefore the only honest resynchronization
        // point at which the old wire slot may be released.
        match self.in_flight.take() {
            // An unconfirmed push may not have landed, so the guest's text is
            // re-learned.
            Some((ServiceReq::ClipPush(text), _)) => {
                self.last_synced.rebaseline = true;
                let is_push = |req: &ServiceReq| matches!(req, ServiceReq::ClipPush(_));
                if !self.queue.iter().any(is_push) {
                    self.queue.push_front(ServiceReq::ClipPush(text));
                }
            }
            // The old agent may have truncated the file before it stopped.
            // Resend it, so that copy ends as a write or a refusal and is
            // never taken for a guest edit.
            Some((put @ ServiceReq::SharePut { .. }, _)) => {
                self.queue.push_front(rewind_share_put_for_retransmit(put));
            }
            _ => {}
        }
        self.get_accum = None;
        self.out_accum = None;
    }
}
