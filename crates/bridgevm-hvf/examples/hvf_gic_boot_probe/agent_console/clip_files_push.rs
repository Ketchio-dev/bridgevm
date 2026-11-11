//! Finder file copies to Windows, with an agent that answered CLIPCAPS with
//! CLIPSETFILES. Each file of the copy is PUT below `<clipdir>\files\<folder>`,
//! then `CLIPSETFILES <folder> <b64(list)> <b64(text)|->` makes the copied items
//! an Explorer copy, with the copy's names as its text. The list has one line
//! per entry, `<isDir>|<path in the folder>`; the agent creates the folders.
//!
//! The folder is named by the copy's digest, not its number: the guest keeps
//! `<clipdir>\files\` across probe runs, whose copy numbers start over, and a
//! folder that only ever receives one tree cannot add stale files to a paste.
//!
//! Each file is read a chunk at a time when it is sent, from a descriptor opened
//! without following links, and only while it has the size and times the copy's
//! walk found (host_pasteboard_files.rs): a file changed since ends the push.
//!
//! A copy within the old limits (`CLIP_FILES_MAX_*`) goes with the chunked PUT
//! a clipboard image uses, and the push holds the wire, as an image push does.
//! A larger one, to an agent whose READY carries `bigfile`, goes with the
//! shared folder's large PUT (LPUTBEG/LPUTDATA/LPUTEND, share_big_put.rs): each
//! file lands at its name only whole, the agent refuses one its disk has no room
//! for, and every step is its own request, so clipboard polls, control and
//! shared-folder traffic go between the chunks. The agent has one large PUT, so
//! a shared-folder one and a push's file never overlap (service_pump).
//!
//! A newer Mac copy ends the push at its next reply, and a newer guest copy
//! (seen a little after the Mac copy, clip_sync.rs) ends a large one; a file
//! ended halfway gets LPUTABORT, which deletes its temporary. An agent restart
//! starts the push over. The guest's next CLIPXF lists the files below
//! `<clipdir>\files\`: that is this push coming back, which clip_files.rs learns
//! and never pulls.

use super::*;
use host_pasteboard::{open_source, HostFileCopy, HostFileSource};
use std::os::unix::fs::FileExt as _;

/// A guest copy polled this long after the Mac copy is newer than it, and ends
/// its large push; one polled sooner may predate it, and the Mac copy wins.
pub(super) const CLIP_FILES_PUSH_GRACE: Duration = Duration::from_secs(2);

/// A Mac file copy on its way to the guest clipboard.
#[derive(Clone)]
pub(super) struct ClipFilesPush {
    /// The Mac copy's number, for the lines.
    copy: u64,
    /// The guest folder below `<clipdir>\files\`, from the copy's digest
    /// (63 bits: at most 19 digits, as the agent takes).
    folder: u64,
    files: HostFileCopy,
    /// The copy's text (its names, LF), set together with the files.
    text: Option<String>,
    /// The entry being PUT; at the end CLIPSETFILES.
    entry: usize,
    /// The chunk of the entry on the wire or next to go (`Chunk`).
    next_chunk: usize,
    /// The step on the wire, or the next one to send.
    pub(super) phase: ClipImagePhase,
    /// Large PUTs, one request per step; else the chunked PUT holding the wire.
    pub(super) large: bool,
    /// When the Mac copy reached the service.
    pub(super) since: Instant,
}

impl ClipFilesPush {
    pub(super) fn new(copy: u64, files: HostFileCopy, text: Option<String>, large: bool, since: Instant) -> Self {
        let folder = u64::from_be_bytes(files.digest[..8].try_into().unwrap()) >> 1;
        let mut push = Self {
            copy,
            folder,
            files,
            text,
            entry: 0,
            next_chunk: 0,
            phase: ClipImagePhase::Beg,
            large,
            since,
        };
        push.rewind();
        push
    }

    /// From the first file again; CLIPSETFILES at once for folders alone.
    pub(super) fn rewind(&mut self) {
        self.entry = self.next_file(0);
        self.next_chunk = 0;
        self.phase = if self.entry < self.files.entries.len() { ClipImagePhase::Beg } else { ClipImagePhase::Set };
    }

    /// A large push past its file's LPUTBEG and before its end: the guest holds a temporary.
    pub(super) fn holds_guest_file(&self) -> bool {
        self.large && matches!(self.phase, ClipImagePhase::Chunk | ClipImagePhase::End)
    }

    fn next_file(&self, from: usize) -> usize {
        let entries = &self.files.entries;
        (from..entries.len()).find(|index| entries[*index].file.is_some()).unwrap_or(entries.len())
    }

    fn source(&self) -> &HostFileSource {
        self.files.entries[self.entry].file.as_ref().expect("a file entry")
    }

    fn chunk_bytes(&self, put_chunk: usize) -> usize {
        if self.large {
            SHARE_PUT_DATA_CHUNK_BYTES
        } else {
            put_chunk
        }
    }

    fn chunks(&self, chunk: usize) -> usize {
        self.source().bytes().div_ceil(chunk as u64) as usize
    }

    /// The step after the one just acknowledged; false once CLIPSETFILES is.
    fn advance(&mut self, chunk: usize) -> bool {
        self.phase = match self.phase {
            ClipImagePhase::Beg | ClipImagePhase::Chunk => {
                if self.phase == ClipImagePhase::Chunk {
                    self.next_chunk += 1;
                }
                if self.next_chunk < self.chunks(chunk) {
                    ClipImagePhase::Chunk
                } else {
                    ClipImagePhase::End
                }
            }
            ClipImagePhase::End => {
                self.entry = self.next_file(self.entry + 1);
                self.next_chunk = 0;
                if self.entry == self.files.entries.len() {
                    ClipImagePhase::Set
                } else {
                    ClipImagePhase::Beg
                }
            }
            ClipImagePhase::Set => return false,
        };
        true
    }

    /// The entry's next chunk, read now from a file still as the walk found it.
    fn read_chunk(&self, chunk: usize) -> std::io::Result<Vec<u8>> {
        let source = self.source();
        let file = open_unchanged(source)?;
        let start = self.next_chunk as u64 * chunk as u64;
        let mut bytes = vec![0; (source.bytes() - start).min(chunk as u64) as usize];
        file.read_exact_at(&mut bytes, start)?;
        Ok(bytes)
    }
}

/// The source file, when it still has the size and times the walk found.
fn open_unchanged(source: &HostFileSource) -> std::io::Result<std::fs::File> {
    let (file, version) = open_source(&source.path)?;
    if version != source.version {
        return Err(std::io::Error::other("changed"));
    }
    Ok(file)
}

impl AgentConsoleHarness {
    /// A Finder copy within the agent's limits: its files and its names in one
    /// push, after any older push still queued is dropped.
    pub(super) fn enqueue_host_files(&mut self, text: Option<String>, files: HostFileCopy, now: Instant) {
        let text = self.host_text_that_fits(text, now);
        self.drop_queued_host_pushes();
        let large = self.share_big && over_old_limits(files.entries.len(), files.bytes);
        let push = ClipFilesPush::new(self.last_synced.host_copy, files, text, large, now);
        self.queue.push_back(ServiceReq::ClipFilesPush(push));
    }

    /// The wire line of the push's current step; false drops the push.
    pub(super) fn write_clip_files_push_line(&mut self, push: &ClipFilesPush, now: Instant) -> bool {
        let chunk = push.chunk_bytes(self.share_put_chunk_bytes());
        let Some(dir) = self.clip_images.as_ref().map(|images| images.dir.clone()) else {
            return false;
        };
        let (out, crlf) = (&mut self.service_line_scratch, &mut self.clip_crlf_scratch);
        if let Err(error) = write_files_push_step(out, crlf, self.put_data, &dir, push, chunk) {
            self.end_files_push(push, &error, now);
            return false;
        }
        true
    }

    /// The (L)PUTs, then CLIPSETFILES, one reply at a time.
    pub(super) fn handle_clip_files_push_reply(&mut self, line: &str, now: Instant, send: &mut dyn FnMut(&[u8])) {
        let newer = self.queue.iter().any(is_host_push);
        let put_chunk = self.share_put_chunk_bytes();
        let Some((ServiceReq::ClipFilesPush(push), sent_at)) = self.in_flight.as_mut() else {
            return;
        };
        let chunk = push.chunk_bytes(put_chunk);
        let (error, acked) = match (push.phase, push.large) {
            (ClipImagePhase::Beg, false) => ("ERR PUTBEG", line == "OK PUTBEG"),
            (ClipImagePhase::Beg, true) => ("ERR LPUTBEG", line == "OK LPUTBEG"),
            (ClipImagePhase::Chunk, large) => {
                let ok = if large { "OK LPUTDATA " } else { "OK PUTCHUNK " };
                let seq = line.strip_prefix(ok).and_then(|seq| seq.parse::<usize>().ok());
                (if large { "ERR LPUTDATA" } else { "ERR PUTCHUNK" }, seq == Some(push.next_chunk))
            }
            (ClipImagePhase::End, large) => (if large { "ERR LPUTEND" } else { "ERR PUTEND" }, line.starts_with("PUTOK ")),
            (ClipImagePhase::Set, _) => ("ERR CLIPSETFILES", line.starts_with("OK CLIPSETFILES")),
        };
        if !acked {
            // An agent that lost the verb answers OUT; any other line is not ours. The agent
            // deletes a large PUT's temporary on its own error.
            let refused = line.starts_with("OUT ") && (push.phase == ClipImagePhase::Set || push.large);
            if line.starts_with(error) || refused {
                println!("BVAGENT CLIPSYNC host->guest files-error {line}");
                if push.large && line.ends_with(" disk-full") {
                    let (entries, bytes) = (push.files.entries.len(), push.files.bytes);
                    let (copy, t) = (push.copy, self.t_ms(now));
                    clip_line(format!(
                        "BVAGENT CLIPSYNC skip host-files guest-full files={entries} bytes={bytes} copy={copy} t={t}"
                    ));
                }
                self.last_synced.rebaseline = true;
                self.in_flight = None;
            }
            return;
        }
        let dir = self.clip_images.as_ref().map(|images| images.dir.clone());
        let Some(dir) = dir.filter(|_| push.phase == ClipImagePhase::Set || !newer) else {
            // A newer Mac copy waits, or images went off: the guest clipboard is as it was. A
            // file just begun or partly sent leaves its temporary to LPUTABORT.
            let Some((ServiceReq::ClipFilesPush(push), _)) = self.in_flight.take() else {
                return;
            };
            if push.large && push.phase != ClipImagePhase::End {
                self.queue.push_front(ServiceReq::ClipFilesAbort);
            }
            return;
        };
        *sent_at = now;
        if push.advance(chunk) {
            if push.large {
                // Its own request again: what waits goes first (service_queue.rs).
                let Some((push, _)) = self.in_flight.take() else {
                    return;
                };
                self.queue.push_back(push);
                return;
            }
            self.service_line_scratch.clear();
            let (out, crlf) = (&mut self.service_line_scratch, &mut self.clip_crlf_scratch);
            if let Err(error) = write_files_push_step(out, crlf, self.put_data, &dir, push, chunk) {
                let Some((ServiceReq::ClipFilesPush(push), _)) = self.in_flight.take() else {
                    return;
                };
                return self.end_files_push(&push, &error, now);
            }
            self.last_send = Some(now);
            send(self.service_line_scratch.as_bytes());
            return;
        }
        let Some((ServiceReq::ClipFilesPush(push), _)) = self.in_flight.take() else {
            return;
        };
        self.last_synced.text = push.text.as_deref().map(normalize_clip);
        self.last_synced.image = None;
        self.last_synced.html = None;
        // The guest holds a file drop now, which no text copy matches.
        self.last_synced.files = Some(push.files.digest);
        self.last_synced.rebaseline = false;
        self.last_synced.echo = true;
        clip_line(format!(
            "BVAGENT CLIPSYNC host->guest files={} bytes={} copy={} t={}",
            push.files.entries.len(),
            push.files.bytes,
            push.copy,
            self.t_ms(now)
        ));
    }

    /// A push whose file changed or went away: the guest clipboard stays as it was, and a large
    /// file begun gets LPUTABORT.
    fn end_files_push(&mut self, push: &ClipFilesPush, error: &std::io::Error, now: Instant) {
        println!("BVAGENT CLIPSYNC host->guest files-error {error} t={}", self.t_ms(now));
        self.last_synced.rebaseline = true;
        if push.holds_guest_file() {
            self.queue.push_front(ServiceReq::ClipFilesAbort);
        }
    }

    /// A newer guest copy ends the large push under way (clip_sync.rs): the Mac copy never
    /// reaches the guest clipboard, and a file partly sent gets LPUTABORT.
    pub(super) fn end_large_files_push(&mut self, now: Instant) {
        let mut abort = false;
        self.queue.retain(|req| match req {
            ServiceReq::ClipFilesPush(push) if push.large => {
                abort |= push.holds_guest_file();
                false
            }
            _ => true,
        });
        if abort {
            self.queue.push_front(ServiceReq::ClipFilesAbort);
        }
        println!("BVAGENT CLIPSYNC host->guest files-ended guest-copy t={}", self.t_ms(now));
    }

    /// When the Mac copy behind a large push under way reached the service, if that push is
    /// the only Mac copy pending.
    pub(super) fn large_files_push_since(&self) -> Option<Instant> {
        let mut since = None;
        for req in self.queue.iter().chain(self.in_flight.iter().map(|(req, _)| req)) {
            match req {
                ServiceReq::ClipFilesPush(push) if push.large => since = Some(push.since),
                req if is_host_push(req) => return None,
                _ => {}
            }
        }
        since
    }

    /// The agent's one large PUT holds a push's file (or its LPUTABORT is pending): no
    /// shared-folder large PUT step may go now.
    pub(super) fn clip_large_put_open(&self) -> bool {
        let begun = matches!(
            &self.in_flight,
            Some((ServiceReq::ClipFilesPush(push), _)) if push.large && push.phase == ClipImagePhase::Beg
        );
        begun
            || self.any_pending(|req| match req {
                ServiceReq::ClipFilesPush(push) => push.holds_guest_file(),
                ServiceReq::ClipFilesAbort => true,
                _ => false,
            })
    }

    /// A large copy's step that had no reply: the copy ends (a resend would repeat a chunk), and
    /// a file the guest may hold gets LPUTABORT.
    pub(super) fn stalled_large_clip_files(&mut self, req: ServiceReq, now: Instant) {
        match req {
            ServiceReq::ClipFilesPull { .. } => self.fail_files_pull("stalled", now),
            ServiceReq::ClipFilesPush(push) => {
                println!("BVAGENT CLIPSYNC host->guest files-error stalled t={}", self.t_ms(now));
                self.last_synced.rebaseline = true;
                if push.phase != ClipImagePhase::Set {
                    self.queue.push_front(ServiceReq::ClipFilesAbort);
                }
            }
            _ => {}
        }
    }

    /// LPUTABORT's reply (or the error of an agent without it) ends it.
    pub(super) fn handle_clip_files_abort_reply(&mut self, line: &str) {
        if line == "OK LPUTABORT" || line.starts_with("ERR") || line.starts_with("OUT ") {
            self.complete_in_flight();
        }
    }
}

/// The push's current step: (L)PUTBEG, a chunk read now, (L)PUTEND or CLIPSETFILES. Err when
/// the file cannot be read as the walk found it.
fn write_files_push_step(
    out: &mut String,
    crlf: &mut String,
    put_data: bool,
    dir: &str,
    push: &ClipFilesPush,
    chunk: usize,
) -> std::io::Result<()> {
    match push.phase {
        ClipImagePhase::Set => {
            let list: String = push
                .files
                .entries
                .iter()
                .map(|entry| format!("{}|{}\n", u8::from(entry.file.is_none()), entry.rel))
                .collect();
            let _ = write!(out, "CLIPSETFILES {} ", push.folder);
            base64_encode_into(list.as_bytes(), out);
            match push.text.as_deref() {
                Some(text) => {
                    out.push(' ');
                    to_guest_crlf_into(text, crlf);
                    base64_encode_into(crlf.as_bytes(), out);
                }
                None => out.push_str(" -"),
            }
            out.push('\n');
        }
        ClipImagePhase::Beg => {
            let path = format!("{dir}\\files\\{}\\{}", push.folder, push.files.entries[push.entry].rel);
            out.push_str(if push.large { "LPUTBEG " } else { "PUTBEG " });
            base64_encode_into(path.as_bytes(), out);
            let bytes = push.source().bytes();
            if push.large {
                let _ = writeln!(out, " {bytes}");
            } else {
                let _ = writeln!(out, " {bytes} {}", push.chunks(chunk));
            }
        }
        ClipImagePhase::Chunk => {
            let bytes = push.read_chunk(chunk)?;
            if push.large {
                let _ = writeln!(out, "LPUTDATA {} {}", push.next_chunk, bytes.len().div_ceil(3) * 4);
                base64_encode_into(&bytes, out);
                out.push('\n');
            } else {
                put_chunk_line_into(out, put_data, push.next_chunk, &bytes);
            }
        }
        ClipImagePhase::End => {
            // The file is the one the walk found up to its last byte.
            open_unchanged(push.source())?;
            let verb = if push.large { "LPUTEND" } else { "PUTEND" };
            let _ = writeln!(out, "{verb} {}", push.chunks(chunk));
        }
    }
    Ok(())
}

/// A copy past what moves with whole-file PUTs and GETs: it needs an agent with `bigfile`.
pub(super) fn over_old_limits(entries: usize, bytes: u64) -> bool {
    entries > CLIP_FILES_MAX_ENTRIES || bytes > CLIP_FILES_MAX_BYTES
}

/// A Mac push of any kind: text, image or files.
pub(super) fn is_host_push(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::ClipPush(..) | ServiceReq::ClipImagePush(_) | ServiceReq::ClipFilesPush(_))
}

/// A step of a clipboard file copy past the old limits: background traffic, as a shared-folder
/// file's (service_queue.rs). Its LPUTABORT goes first, as clipboard traffic.
pub(super) fn is_large_clip_files_req(req: &ServiceReq) -> bool {
    match req {
        ServiceReq::ClipFilesPull { ranged } => *ranged,
        ServiceReq::ClipFilesPush(push) => push.large,
        _ => false,
    }
}
