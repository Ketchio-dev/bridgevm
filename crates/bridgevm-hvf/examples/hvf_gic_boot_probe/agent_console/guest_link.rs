//! Windows links for the Mac browser (the app's "Windows 링크를 Mac에서 열기").
//!
//! The app registers a per-user BridgeVM browser in the guest and sends
//! `LINKWATCH`. The agent then writes `OPENURL <b64(utf8 url)>` between replies
//! for each link the handler appended (bvagent.ps1, link watch). This checks the
//! guest's bytes and prints one line the app reads from run.log:
//! `BVAGENT OPENURL at=<unix_ms> <b64>`, or `BVAGENT OPENURL refused reason=<r>`
//! without the guest's text. The app checks the URL again before it opens it.

use std::time::{SystemTime, UNIX_EPOCH};

use super::*;

pub(super) const WATCH_VERB: &str = "LINKWATCH";
/// The longest URL taken, in UTF-8 bytes; the handler and the app use the same.
const MAX_URL_BYTES: usize = 2048;
const MAX_URL_BASE64: usize = MAX_URL_BYTES.div_ceil(3) * 4;

/// The run.log line for the base64 after `OPENURL `, stamped `unix_ms`.
pub(super) fn open_url_line(b64: &str, unix_ms: u128) -> String {
    match check(b64) {
        Ok(()) => format!("BVAGENT OPENURL at={unix_ms} {b64}"),
        Err(reason) => format!("BVAGENT OPENURL refused reason={reason}"),
    }
}

fn check(b64: &str) -> Result<(), &'static str> {
    // A guest line may be up to MAX_AGENT_LINE_BYTES; nothing longer than a URL is decoded.
    if b64.len() > MAX_URL_BASE64 {
        return Err("too-long");
    }
    let bytes = base64_decode(b64).map_err(|_| "base64")?;
    let url = std::str::from_utf8(&bytes).map_err(|_| "utf8")?;
    if bytes.len() > MAX_URL_BYTES {
        return Err("too-long");
    }
    if url.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err("character");
    }
    let scheme_end = url.find("://").ok_or("scheme")?;
    let scheme = &url[..scheme_end];
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || url.len() == scheme_end + 3
    {
        return Err("scheme");
    }
    Ok(())
}

#[cfg(test)]
#[path = "guest_link_tests.rs"]
mod tests;

impl AgentConsoleHarness {
    /// An `OPENURL` line between replies: printed, never matched to the request in flight.
    pub(super) fn guest_link(&self, line: &str) -> bool {
        let Some(b64) = line.strip_prefix("OPENURL ") else {
            return false;
        };
        let unix_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_millis());
        println!("{}", open_url_line(b64, unix_ms));
        true
    }

    /// A restarted agent has no link watch: ask again if this harness had asked.
    pub(super) fn resume_link_watch(&mut self) {
        let watch = |req: &ServiceReq| matches!(req, ServiceReq::Ctl(c) if c == WATCH_VERB);
        if self.link_watch && !self.queue.iter().any(watch) {
            self.queue.push_back(ServiceReq::Ctl(WATCH_VERB.to_owned()));
        }
    }
}
