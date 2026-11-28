//! "Share clipboard" switched while Windows runs: the app writes `BVCLIPSYNC off` or `BVCLIPSYNC on`
//! to agent.ctl. Before, the setting was only the start's, so stopping sharing (to copy a password
//! on the Mac, say) meant shutting Windows down.

use super::*;

/// The switch an agent.ctl line asks for, if it is one.
pub(super) fn clip_switch(command: &str) -> Option<bool> {
    match command.strip_prefix("BVCLIPSYNC ")? {
        "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

impl AgentConsoleHarness {
    /// Off: no new copy crosses either way. Mac copies are read and dropped (clip_sync.rs), Windows
    /// is no longer asked for its clipboard, and the copies queued to start are dropped; one under
    /// way ends. On: sharing starts as at the service start.
    pub(super) fn switch_clipsync(&mut self, on: bool) {
        if on == self.clipsync {
            return;
        }
        self.clipsync = on;
        println!("BVAGENT CLIPSYNC {}", if on { "on" } else { "off" });
        if !on {
            self.queue
                .retain(|req| !matches!(req, ServiceReq::ClipPoll | ServiceReq::ClipPush(..) | ServiceReq::ClipCaps));
            return;
        }
        if self.pasteboard.is_none() {
            let poll_ms = super::harness_protocol::host_pasteboard_poll_ms(self.clip_interval.as_millis() as u64);
            self.pasteboard = Some(HostPasteboard::spawn(poll_ms));
        }
        self.restart_clip_sync();
    }

    /// As at the service start: CLIPCAPS first, nothing known of Windows' clipboard, and the Mac's
    /// copy goes again.
    pub(super) fn restart_clip_sync(&mut self) {
        self.queue.retain(|req| !matches!(req, ServiceReq::ClipCaps));
        self.queue.push_front(ServiceReq::ClipCaps);
        let synced = &mut self.last_synced;
        (synced.text, synced.image, synced.html, synced.files) = (None, None, None, None);
        if let Some(pasteboard) = self.pasteboard.as_ref() {
            pasteboard.report_again();
        }
    }
}
