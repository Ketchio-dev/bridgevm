//! Shared-folder files larger than one range (`SHARE_RANGE_BYTES`) move in pieces, for an agent
//! whose READY carries `bigfile`.
//!
//! Windows to Mac: `GETRANGE <b64(path)> <offset> <length>` answers GETBEG with the file's size
//! and time as they were after the read, then GETCHUNKs and GETEND. Each range is its own
//! lockstep request, so clipboard and control traffic still go first between ranges. A file
//! whose first range holds all of it takes the in-memory path (share_get_publish.rs); a larger
//! one goes to a writer thread (share_stage_writer.rs) chunk by chunk, so memory stays near two
//! ranges and vCPU 0's thread only frames lines. Every range must report the size and time
//! the guest listing gave the file: otherwise the file changed while it was copied, the copy
//! is abandoned and the next listing fetches it again. The Mac copy appears at its name only
//! complete and F_FULLFSYNCed. An agent restart, a stall or a guest error abandons the copy and
//! its temporary; a probe killed mid-copy leaves a temporary the next host scan removes.
//!
//! While a large copy runs no other shared-folder request goes out, as before.

use super::host_destination::Destination;
use super::share_stage_writer::{decoded_len, FromWriter, StageWriter};
use super::*;

/// The most one GETRANGE asks for.
pub(in crate::agent_console) const SHARE_RANGE_BYTES: u64 = 8 << 20;
/// A large copy to the Mac needs this much free space beyond its own size.
pub(in crate::agent_console) const SHARE_FREE_MARGIN: u64 = 2 << 30;

pub(in crate::agent_console) enum BigTransfer {
    Get(BigGet),
    Put(super::share_big_put::BigPut),
}

pub(in crate::agent_console) struct BigGet {
    pub(in crate::agent_console) name: String,
    size: u64,
    mtime: String,
    /// The offset the next range starts at.
    next: u64,
    /// The range being received: GETBEG seen, GETEND not yet.
    range: Option<RangeIn>,
    writer: StageWriter,
    publishing: bool,
    started: Instant,
}

struct RangeIn {
    len: u64,
    seen: u64,
    seq: u64,
}

/// A GETRANGE reply's GETBEG: `<b64(path)> <bytes> <nchunks> <offset> <size> <mtime>`.
pub(in crate::agent_console) fn parse_range_beg(rest: &str) -> Option<(String, u64, GetRange)> {
    let mut it = rest.split(' ');
    let path = String::from_utf8(base64_decode(it.next()?).ok()?).ok()?;
    let len = it.next()?.parse().ok()?;
    let _chunks: u64 = it.next()?.parse().ok()?;
    let offset = it.next()?.parse().ok()?;
    let size = it.next()?.parse().ok()?;
    let mtime = it.next()?.to_string();
    if it.next().is_some() || mtime.is_empty() {
        return None;
    }
    Some((path, len, GetRange { offset, size, mtime }))
}

impl AgentConsoleHarness {
    pub(in crate::agent_console) fn big_share_active(&self) -> bool {
        self.share.as_ref().is_some_and(|share| share.big.is_some())
    }

    fn big_get(&mut self) -> Option<&mut BigGet> {
        match self.share.as_mut()?.big.as_mut()? {
            BigTransfer::Get(get) => Some(get),
            BigTransfer::Put(_) => None,
        }
    }

    /// `GETRANGE <b64(path)> <offset> <length>` for the request's file.
    pub(in crate::agent_console) fn write_share_range_line(&mut self, name: &str, offset: u64) -> bool {
        let Some(share) = self.share.as_ref() else {
            return false;
        };
        let len = match share.big.as_ref() {
            Some(BigTransfer::Get(get)) if get.name == name && offset < get.size => {
                (get.size - offset).min(SHARE_RANGE_BYTES)
            }
            None if offset == 0 => SHARE_RANGE_BYTES,
            _ => return false,
        };
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        self.get_accum = None;
        self.service_line_scratch.push_str("GETRANGE ");
        base64_encode_into(self.share_guest_path_scratch.as_bytes(), &mut self.service_line_scratch);
        let _ = writeln!(&mut self.service_line_scratch, " {offset} {len}");
        true
    }

    /// Whether a fetch of `name` (listed at `size`) would need more space than the Mac has:
    /// then it is skipped, reported once per listed version.
    pub(in crate::agent_console) fn share_space_refuses(&mut self, name: &str, size: u64) -> bool {
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        let Ok(root) = share.checked_root() else {
            return false;
        };
        let free = super::host_destination::free_bytes(&root).unwrap_or(u64::MAX);
        if free >= size.saturating_add(SHARE_FREE_MARGIN) {
            return false;
        }
        share.report_low_space(name, size);
        true
    }

    /// A GET or GETRANGE reply line while a share fetch is in flight that belongs to a large
    /// copy; false leaves the line to the whole-file path.
    pub(in crate::agent_console) fn handle_big_get_line(&mut self, line: &str, now: Instant) -> bool {
        let (name, range_req) = match self.in_flight.as_ref() {
            Some((ServiceReq::ShareGet { name }, _)) => (name.clone(), false),
            Some((ServiceReq::ShareRange { name, .. }, _)) => (name.clone(), true),
            _ => return false,
        };
        if let Some(rest) = line.strip_prefix("GETBEG ") {
            return self.big_get_begin(&name, range_req, rest, now);
        }
        let mine = self
            .big_get()
            .is_some_and(|get| get.name == name && get.range.is_some());
        // An abandoned range is still read to its GETEND, which frees the wire.
        if !mine && !range_req {
            return false;
        }
        if let Some(rest) = line.strip_prefix("GETCHUNK ") {
            if mine {
                self.big_get_chunk(rest, now);
            }
            return true;
        }
        if line.starts_with("GETEND ") {
            if mine {
                self.big_get_range_end(now);
            }
            self.complete_in_flight();
            return true;
        }
        if line.starts_with("ERR") {
            println!("BVAGENT SHARE get-error {name} {line}");
            self.abort_big_share("guest-error", now);
            self.complete_in_flight();
            return true;
        }
        false
    }

    fn big_get_begin(&mut self, name: &str, range_req: bool, rest: &str, now: Instant) -> bool {
        let Some((path, len, range)) = parse_range_beg(rest) else {
            if range_req {
                self.abort_big_share("bad-reply", now);
            }
            return range_req;
        };
        let Some(share) = self.share.as_ref() else {
            return range_req;
        };
        write_share_guest_path_into(share, name, &mut self.share_guest_path_scratch);
        let path_ok = path == self.share_guest_path_scratch;
        if !range_req {
            // A file the first range holds whole is fetched as before.
            if !path_ok || range.offset != 0 || range.size <= len || self.big_share_active() {
                return false;
            }
            return self.start_big_get(name, len, range, now);
        }
        let Some(get) = self.big_get().filter(|get| get.name == name) else {
            return true;
        };
        let expected = (get.size - get.next.min(get.size)).min(SHARE_RANGE_BYTES);
        let same = range.offset == get.next && range.size == get.size && range.mtime == get.mtime;
        if path_ok && same && len == expected && get.range.is_none() {
            get.range = Some(RangeIn { len, seen: 0, seq: 0 });
        } else {
            println!("BVAGENT SHARE get-changed {name} t={}", self.t_ms(now));
            self.abort_big_share("changed", now);
        }
        true
    }

    /// The first range of a file larger than one: begin its copy, or say why not (false: the
    /// whole-file path then reads the reply out without writing anything).
    fn start_big_get(&mut self, name: &str, len: u64, range: GetRange, now: Instant) -> bool {
        let t = self.t_ms(now);
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        if range.size > share.engine.max_bytes() {
            if let GuestVersion::Skip(reason) =
                share.engine.on_guest_version(name.to_string(), range.size, 0, None)
            {
                print_share_skip(name, reason);
            }
            return false;
        }
        if share.engine.listed_file(name) != Some((range.size, range.mtime.as_str()))
            || len != range.size.min(SHARE_RANGE_BYTES)
        {
            println!("BVAGENT SHARE get-changed {name} t={t}");
            return false;
        }
        if self.share_space_refuses(name, range.size) {
            return false;
        }
        let Some(share) = self.share.as_mut() else {
            return false;
        };
        let Ok(root) = share.checked_root() else {
            println!("BVAGENT SHARE get-deferred <root-unavailable>");
            return false;
        };
        share.forget_host_walk();
        let marks = share.sync_marks().map(|marks| (marks, name.to_string()));
        let writer = Destination::from_root(root, name, true)
            .and_then(|destination| StageWriter::start(destination, range.size, marks));
        let writer = match writer {
            Ok(writer) => writer,
            Err(error) => {
                self.big_get_write_failed(name, range.size, &error);
                return false;
            }
        };
        println!("BVAGENT SHARE transfer guest->host {name} bytes={} t={t}", range.size);
        share_state_mut(&mut self.share).big = Some(BigTransfer::Get(BigGet {
            name: name.to_string(),
            size: range.size,
            mtime: range.mtime,
            next: 0,
            range: Some(RangeIn { len, seen: 0, seq: 0 }),
            writer,
            publishing: false,
            started: now,
        }));
        true
    }

    fn big_get_chunk(&mut self, rest: &str, now: Instant) {
        let Some(get) = self.big_get() else {
            return;
        };
        let Some(range) = get.range.as_mut() else {
            return;
        };
        let parsed = rest.split_once(' ').and_then(|(seq, b64)| {
            let n = decoded_len(b64)?;
            (seq.parse() == Ok(range.seq) && range.seen + n <= range.len).then_some((b64, n))
        });
        match parsed {
            Some((b64, n)) => {
                range.seen += n;
                range.seq += 1;
                get.writer.chunk(b64.to_string(), n);
            }
            None => self.abort_big_share("bad-chunk", now),
        }
    }

    fn big_get_range_end(&mut self, now: Instant) {
        let Some(get) = self.big_get() else {
            return;
        };
        let Some(range) = get.range.take() else {
            return;
        };
        if range.seen != range.len {
            return self.abort_big_share("short", now);
        }
        get.next += range.len;
    }

    /// The writer's answers, then the next range once the writer has caught up.
    pub(in crate::agent_console) fn advance_big_share(&mut self, now: Instant) {
        if matches!(self.share.as_ref().and_then(|share| share.big.as_ref()), Some(BigTransfer::Put(_))) {
            return self.advance_big_put(now);
        }
        while let Some(answer) = self.big_get().and_then(|get| get.writer.poll()) {
            match answer {
                FromWriter::Hashed(hash) => self.big_get_hashed(hash),
                FromWriter::Published(result) => self.big_get_published(result, now),
                FromWriter::Failed(error) if error.kind() == std::io::ErrorKind::InvalidData => {
                    self.abort_big_share("bad-chunk", now)
                }
                FromWriter::Failed(error) => {
                    let Some((name, size)) = self.big_get().map(|get| (get.name.clone(), get.size))
                    else {
                        return;
                    };
                    share_state_mut(&mut self.share).big = None;
                    self.big_get_write_failed(&name, size, &error);
                }
            }
        }
        let range_out = self.any_pending(|req| matches!(req, ServiceReq::ShareRange { .. }));
        let Some(get) = self.big_get() else {
            return;
        };
        if get.range.is_none()
            && !get.publishing
            && get.next < get.size
            && get.writer.backlog <= SHARE_RANGE_BYTES
            && !range_out
        {
            let request = ServiceReq::ShareRange {
                name: get.name.clone(),
                offset: get.next,
            };
            self.queue.push_back(request);
        }
    }

    fn big_get_hashed(&mut self, hash: u64) {
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let Some(BigTransfer::Get(get)) = share.big.as_mut() else {
            return;
        };
        let name = get.name.clone();
        // A preferred guest edit goes only over the Mac file it was preferred to (share_synced.rs).
        let recovery = share.engine.recovery_copy(&name);
        let mac = share.fetch_mac.get(&name).copied().or_else(|| share.engine.preferred_mac_stat(&name).map(Some));
        match share.engine.on_guest_version(name.clone(), get.size, hash, None) {
            GuestVersion::AlreadySynced => {
                share.fetch_mac.remove(&name);
                share.big = None;
            }
            GuestVersion::Skip(reason) => {
                share.big = None;
                print_share_skip(&name, reason);
            }
            GuestVersion::WriteHost => {
                let mtime = (share.engine.guest_mtime(&name))
                    .and_then(share_sync::share_mtime::guest_mtime_ms)
                    .and_then(share_sync::share_mtime::system_time);
                get.writer.publish(mtime, mac, recovery);
                get.publishing = true;
            }
        }
    }

    fn big_get_published(&mut self, result: std::io::Result<Option<u128>>, now: Instant) {
        let t = self.t_ms(now);
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let Some(BigTransfer::Get(get)) = share.big.take() else {
            return;
        };
        share.forget_host_walk();
        // Marked on disk by the writer thread before it published (share_stage_writer.rs).
        if !share.engine.recovery_copy(&get.name) { share.engine.mark_settling(&get.name, now); }
        match result {
            Ok(mtime) => {
                share.fetch_mac.remove(&get.name);
                share.engine.on_host_write_succeeded(&get.name, mtime);
                let ms = now.duration_since(get.started).as_millis().max(1);
                println!("BVAGENT SHARE guest->host {} bytes={} t={t}", get.name, get.size);
                println!(
                    "BVAGENT SHARE transfer-done guest->host {} bytes={} ms={ms} mbps={:.1} t={t}",
                    get.name,
                    get.size,
                    get.size as f64 / 1e3 / ms as f64
                );
            }
            Err(error) if super::host_destination::is_mac_changed(&error) => {
                share.fetch_mac.remove(&get.name);
                share.engine.on_host_copy_kept(&get.name);
                println!("BVAGENT SHARE skip {} mac-changed", get.name);
            }
            Err(error) => self.big_get_write_failed(&get.name, get.size, &error),
        }
    }

    /// A Mac-side failure is handled as a failed whole-file write (share_get_publish.rs).
    fn big_get_write_failed(&mut self, name: &str, size: u64, error: &std::io::Error) {
        let Some(share) = self.share.as_mut() else {
            return;
        };
        // The record the failure is noted on (on_guest_file makes it before a whole-file write).
        let _ = share.engine.on_guest_version(name.to_string(), size, 0, None);
        let exhausted = share
            .engine
            .on_host_write_error(name, super::guest_write_refusal::is_persistent(error));
        // A full Mac disk fails these writes; run.log may be on it (host_log).
        bridgevm_hvf::host_log::out(format_args!("BVAGENT SHARE write-refused {name} {error}"));
        if is_out_of_space(error) {
            share.report_low_space(name, size);
        }
        if exhausted {
            self.share = None;
            self.queue.retain(|request| !is_share_req(request));
            bridgevm_hvf::host_log::out("BVAGENT SHARE disabled failed-destination-limit");
        }
    }

    /// Abandon the large copy under way, unless it is already being published (that no longer
    /// needs the agent). Its temporary is removed; the next listing or scan starts it again.
    pub(in crate::agent_console) fn abort_big_share(&mut self, reason: &str, now: Instant) {
        let t = self.t_ms(now);
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let get = match share.big.as_ref() {
            Some(BigTransfer::Get(get)) if get.publishing => return,
            Some(BigTransfer::Get(_)) => match share.big.take() {
                Some(BigTransfer::Get(get)) => get,
                _ => unreachable!("checked"),
            },
            // A stall or another fault asks the agent to delete its temporary; a new agent holds none.
            Some(BigTransfer::Put(_)) => return self.abort_big_put(reason, reason != "agent-restarted", now),
            None => return,
        };
        println!(
            "BVAGENT SHARE get-abort {} {reason} bytes={}/{} t={t}",
            get.name, get.next, get.size
        );
        self.queue
            .retain(|req| !matches!(req, ServiceReq::ShareRange { .. }));
    }

    /// Whether `req` is a fetch whose large copy is under way.
    pub(in crate::agent_console) fn is_big_share_step(&self, req: &ServiceReq) -> bool {
        match req {
            ServiceReq::ShareRange { .. } | ServiceReq::ShareBigPut(_) => true,
            ServiceReq::ShareGet { name } => matches!(
                self.share.as_ref().and_then(|share| share.big.as_ref()),
                Some(BigTransfer::Get(get)) if get.name == *name
            ),
            _ => false,
        }
    }
}

impl ShareState {
    /// `name` (`size` bytes) is not copied to the Mac for want of space: told once per listed
    /// version (the app's low-space notice), checked again at every listing.
    pub(in crate::agent_console) fn report_low_space(&mut self, name: &str, size: u64) {
        let version = (name.to_string(), self.engine.listed_file(name).map(|(_, m)| m.to_string()));
        if self.low_space_seen.len() >= 1024 {
            self.low_space_seen.clear();
        }
        if self.low_space_seen.insert(version) {
            bridgevm_hvf::host_log::out(format_args!("BVAGENT SHARE skip {name} low-space {size}"));
        }
    }
}

/// A write the Mac refused for want of space or quota. APFS refuses writes while it still
/// reports some space free (a 4 MiB write failed with 33.7 MB free), so no precheck catches
/// every such file.
pub(in crate::agent_console) fn is_out_of_space(error: &std::io::Error) -> bool {
    matches!(error.raw_os_error(), Some(libc::ENOSPC | libc::EDQUOT))
}

fn share_state_mut(share: &mut Option<ShareState>) -> &mut ShareState {
    share.as_mut().expect("the share is on")
}

#[cfg(test)]
#[path = "share_big_tests.rs"]
mod tests;
