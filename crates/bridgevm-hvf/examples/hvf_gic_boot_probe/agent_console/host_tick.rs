//! Host-only agent progression on the service thread; the caller collects the wire sends.

use crate::agent_console::*;

impl AgentConsoleHarness {
    /// Take the guest bytes vCPU0 drained (service_thread.rs) and return how
    /// many. A finished harness leaves them, so the drain stops at its bound.
    pub(in crate::agent_console) fn take_inbound(&mut self, rx: &mut Vec<u8>) -> usize {
        if matches!(
            self.state,
            AgentConsoleState::Done | AgentConsoleState::TimedOut
        ) {
            return 0;
        }
        self.inbound_scratch.clear();
        std::mem::swap(&mut self.inbound_scratch, rx);
        self.inbound_scratch.len()
    }

    /// Process replies and host filesystem work without borrowing a platform or guest RAM.
    pub fn advance(&mut self, now: Instant, send: &mut dyn FnMut(&[u8])) {
        if matches!(
            self.state,
            AgentConsoleState::Done | AgentConsoleState::TimedOut
        ) {
            return;
        }
        self.observe_host_clock();

        let oversized = self.frame_inbound_lines();
        let mut lines = std::mem::take(&mut self.line_scratch);
        for line in lines.drain(..) {
            self.handle_line_with_sender(&line, now, send);
        }
        self.line_scratch = lines;
        if oversized > 0 {
            self.share_listing_line_too_long();
        }

        // Proactively PING while waiting to connect. The agent may have sent
        // its one-shot READY before the guest driver's host-open latched (that
        // write is silently dropped), so waiting passively for READY can
        // deadlock. A PING the agent echoes as PONG breaks that: any inbound
        // line (READY or PONG) advances us to the command phase.
        // The same while a PONG is owed: the PING may have gone to an agent
        // that has restarted since.
        if matches!(
            self.state,
            AgentConsoleState::WaitingReady | AgentConsoleState::WaitingPong
        ) {
            let due = self
                .last_ping
                .is_none_or(|t| now.duration_since(t) >= PING_INTERVAL);
            if due {
                send(b"PING\n");
                self.last_ping = Some(now);
            }
        }

        // Only the resident service's handshake is short; other scripted
        // commands may run for minutes.
        if self.service
            && matches!(self.state, AgentConsoleState::WaitingOut { .. })
            && self
                .out_since
                .is_some_and(|t| now.duration_since(t) >= HANDSHAKE_REPLY_TIMEOUT)
        {
            println!("BVAGENT CMD no reply in {} s: PING again t={}", HANDSHAKE_REPLY_TIMEOUT.as_secs(), self.t_ms(now));
            self.restart_handshake(now, send);
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
            self.service_tick_with_sender(now, send);
        }

        // BVAGENT lines are the greppable live evidence, and run.log is a shell
        // redirect (not a tty) — live-observed to lag by minutes without an
        // explicit flush, which breaks log-tailing proof drivers. Flushing an
        // empty buffer is a no-op, so doing it every tick is cheap.
        use std::io::Write as _;
        let _ = std::io::stdout().flush();
    }
}

#[cfg(test)]
#[path = "host_tick_tests.rs"]
mod tests;
