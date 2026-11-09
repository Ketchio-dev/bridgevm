//! Mac files larger than one range (`SHARE_RANGE_BYTES`) go to an agent with `bigfile` read one
//! 1 MiB chunk at a time from a descriptor opened without following symlinks, never whole.
//!
//! `LPUTBEG`, one `LPUTDATA` per chunk and `LPUTEND` are each their own lockstep request, so
//! control and clipboard traffic go out between chunks (as a Finder drop's). The agent writes
//! them into a temporary of its own and renames it over the target only whole and flushed
//! (bvagent.ps1 "large put"). Before every chunk and before LPUTEND the descriptor's size,
//! modification and change times must be the ones it had at the start: a file changed while it
//! was sent is abandoned with LPUTABORT and sent again from the next host scan. A guest refusal
//! (disk-full, a name Windows rejects) holds the file until the Mac copy changes, as a refused
//! PUT does. An agent restart abandons the file; the new agent's first listing deletes the
//! temporary.

use super::host_destination::snapshot::{version, SourceVersion};
use super::share_big::BigTransfer;
use super::*;
use std::fs::File;
use std::os::unix::fs::FileExt as _;

/// One step on the wire; each has a one-line reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(in crate::agent_console) enum BigPutStep {
    /// `LPUTBEG <b64(path)> <size> <mtime ms>` -> `OK LPUTBEG`.
    Beg,
    /// `LPUTDATA <seq> <chars>` and the payload line -> `OK LPUTDATA <seq>`.
    Data { seq: u64, bytes: Vec<u8> },
    /// `LPUTEND <chunks>` -> `PUTOK <b64(path)> <bytes>`.
    End { chunks: u64 },
    /// `LPUTABORT` -> `OK LPUTABORT`: the guest deletes its temporary.
    Abort,
}

pub(in crate::agent_console) struct BigPut {
    pub(in crate::agent_console) name: String,
    file: File,
    version: SourceVersion,
    pub(in crate::agent_console) size: u64,
    mtime_ms: u128,
    /// Bytes read into chunks so far.
    pub(in crate::agent_console) read: u64,
    seq: u64,
    hash: u64,
    begun: bool,
    ended: bool,
    started: Instant,
}

pub(in crate::agent_console) enum BigPutStart {
    Started,
    /// The first listing showed a guest copy of this size and time (share_adopt.rs).
    Adopted,
    No,
}

/// Open `name` and make it the share's large copy, unless the guest already holds it.
pub(in crate::agent_console) fn start_big_put(share: &mut ShareState, name: &str, now: Instant) -> BigPutStart {
    let Ok((source, _)) = snapshot_source(share, name) else {
        return BigPutStart::No;
    };
    let Ok((file, meta)) = source.open_source() else {
        return BigPutStart::No;
    };
    let Some(mtime_ms) = file_mtime_ms_from_meta(&meta) else {
        return BigPutStart::No;
    };
    let size = meta.len();
    if size > share.engine.max_bytes() {
        print_host_skip_once(share, name, mtime_ms, HostSkipKind::TooLarge, size);
        return BigPutStart::No;
    }
    if share.engine.adopt_listed_version(name, size, mtime_ms, || None) {
        return BigPutStart::Adopted;
    }
    if !share.engine.on_host_push_streamed(name) {
        return BigPutStart::No;
    }
    println!("BVAGENT SHARE transfer host->guest {name} bytes={size}");
    share.big = Some(BigTransfer::Put(BigPut {
        name: name.to_string(),
        file,
        version: version(&meta),
        size,
        mtime_ms,
        read: 0,
        seq: 0,
        hash: share_sync::FNV1A64_START,
        begun: false,
        ended: false,
        started: now,
    }));
    BigPutStart::Started
}

#[derive(Clone, Copy)]
enum StepKind {
    Beg,
    Data(u64),
    End,
    Abort,
}

impl AgentConsoleHarness {
    fn big_put(&mut self) -> Option<&mut BigPut> {
        match self.share.as_mut()?.big.as_mut()? {
            BigTransfer::Put(put) => Some(put),
            _ => None,
        }
    }

    pub(in crate::agent_console) fn write_big_put_line(&mut self, step: &BigPutStep) -> bool {
        let out = &mut self.service_line_scratch;
        match step {
            BigPutStep::Beg => {
                let Some(share) = self.share.as_ref() else {
                    return false;
                };
                let Some(BigTransfer::Put(put)) = share.big.as_ref() else {
                    return false;
                };
                write_share_guest_path_into(share, &put.name, &mut self.share_guest_path_scratch);
                out.push_str("LPUTBEG ");
                base64_encode_into(self.share_guest_path_scratch.as_bytes(), out);
                let _ = writeln!(out, " {} {}", put.size, put.mtime_ms);
            }
            BigPutStep::Data { seq, bytes } => {
                let _ = writeln!(out, "LPUTDATA {seq} {}", bytes.len().div_ceil(3) * 4);
                base64_encode_into(bytes, out);
                out.push('\n');
            }
            BigPutStep::End { chunks } => {
                let _ = writeln!(out, "LPUTEND {chunks}");
            }
            BigPutStep::Abort => out.push_str("LPUTABORT\n"),
        }
        true
    }

    /// The next step once the previous one has its reply: a chunk read now, from a file
    /// that has not changed since the start, or the end.
    pub(in crate::agent_console) fn advance_big_put(&mut self, now: Instant) {
        if self.any_pending(|req| matches!(req, ServiceReq::ShareBigPut(_))) {
            return;
        }
        let t = self.t_ms(now);
        let Some(put) = self.big_put() else {
            return;
        };
        if !put.begun || put.ended {
            return;
        }
        let unchanged = put.file.metadata().is_ok_and(|meta| version(&meta) == put.version);
        if !unchanged {
            println!("BVAGENT SHARE put-changed {} t={t}", put.name);
            return self.abort_big_put("changed", true, now);
        }
        let step = if put.read < put.size {
            let len = (put.size - put.read).min(SHARE_PUT_DATA_CHUNK_BYTES as u64) as usize;
            let mut bytes = vec![0; len];
            if put.file.read_exact_at(&mut bytes, put.read).is_err() {
                return self.abort_big_put("read-error", true, now);
            }
            put.hash = share_sync::fnv1a64_continue(put.hash, &bytes);
            put.read += len as u64;
            put.seq += 1;
            BigPutStep::Data {
                seq: put.seq - 1,
                bytes,
            }
        } else {
            put.ended = true;
            BigPutStep::End { chunks: put.seq }
        };
        self.queue.push_back(ServiceReq::ShareBigPut(step));
    }

    pub(in crate::agent_console) fn handle_big_put_reply(&mut self, line: &str, now: Instant) {
        let kind = match self.in_flight.as_ref() {
            Some((ServiceReq::ShareBigPut(step), _)) => match step {
                BigPutStep::Beg => StepKind::Beg,
                BigPutStep::Data { seq, .. } => StepKind::Data(*seq),
                BigPutStep::End { .. } => StepKind::End,
                BigPutStep::Abort => StepKind::Abort,
            },
            _ => return,
        };
        let (ok, err) = match kind {
            StepKind::Beg => (line == "OK LPUTBEG", "ERR LPUTBEG"),
            StepKind::Data(seq) => (
                line.strip_prefix("OK LPUTDATA ") == Some(seq.to_string().as_str()),
                "ERR LPUTDATA",
            ),
            StepKind::End => (line.starts_with("PUTOK "), "ERR LPUTEND"),
            StepKind::Abort => (line == "OK LPUTABORT", "ERR LPUTABORT"),
        };
        let refused = line.strip_prefix(err).is_some_and(|rest| rest.is_empty() || rest.starts_with(' '));
        if !ok && !refused {
            return;
        }
        self.complete_in_flight();
        if matches!(kind, StepKind::Abort) {
            return;
        }
        if refused {
            return self.big_put_refused(line);
        }
        match kind {
            StepKind::Beg => {
                if let Some(put) = self.big_put() {
                    put.begun = true;
                }
            }
            StepKind::End => self.big_put_done(line, now),
            StepKind::Data(_) | StepKind::Abort => {}
        }
    }

    fn big_put_done(&mut self, line: &str, now: Instant) {
        let t = self.t_ms(now);
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let Some(BigTransfer::Put(put)) = share.big.take() else {
            return;
        };
        let written = line.rsplit(' ').next().and_then(|n| n.parse::<u64>().ok());
        if written != Some(put.size) {
            println!("BVAGENT SHARE put-error {} {line}", put.name);
            share.engine.on_put_rejected(&put.name);
            return;
        }
        share.engine.on_put_ok(put.name.clone(), put.size, put.hash);
        // The Mac file is the one opened at the start (its version was checked before LPUTEND):
        // the next scan finds it synced instead of sending it again.
        share.engine.on_host_write_succeeded(&put.name, Some(put.mtime_ms));
        let ms = now.duration_since(put.started).as_millis().max(1);
        println!("BVAGENT SHARE host->guest {} bytes={} t={t}", put.name, put.size);
        println!(
            "BVAGENT SHARE transfer-done host->guest {} bytes={} ms={ms} mbps={:.1} t={t}",
            put.name,
            put.size,
            put.size as f64 / 1e3 / ms as f64
        );
    }

    fn big_put_refused(&mut self, line: &str) {
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let Some(BigTransfer::Put(put)) = share.big.take() else {
            return;
        };
        println!("BVAGENT SHARE put-error {} {line}", put.name);
        share.engine.on_put_rejected(&put.name);
        self.queue
            .retain(|req| !matches!(req, ServiceReq::ShareBigPut(_)));
    }

    /// Abandon the large PUT; `tell_agent` asks the agent to delete its temporary (not after a
    /// restart: the new agent holds none open and its listing deletes it).
    pub(in crate::agent_console) fn abort_big_put(&mut self, reason: &str, tell_agent: bool, now: Instant) {
        let t = self.t_ms(now);
        let Some(share) = self.share.as_mut() else {
            return;
        };
        let Some(BigTransfer::Put(put)) = share.big.take() else {
            return;
        };
        println!(
            "BVAGENT SHARE put-abort {} {reason} bytes={}/{} t={t}",
            put.name, put.read, put.size
        );
        self.queue
            .retain(|req| !matches!(req, ServiceReq::ShareBigPut(_)));
        if tell_agent {
            self.queue
                .push_back(ServiceReq::ShareBigPut(BigPutStep::Abort));
        }
    }
}

#[cfg(test)]
#[path = "share_big_put_tests.rs"]
mod tests;
