//! Finder file copies to Windows, with an agent that answered CLIPCAPS with
//! CLIPSETFILES. Each file of the copy is PUT below `<clipdir>\files\<folder>`
//! with the chunked PUT a clipboard image uses, then
//! `CLIPSETFILES <folder> <b64(list)> <b64(text)|->` makes the copied items an
//! Explorer copy, with the copy's names as its text. The list has one line per
//! entry, `<isDir>|<path in the folder>`; the agent creates the folders.
//!
//! The folder is named by the copy's digest, not its number: the guest keeps
//! `<clipdir>\files\` across probe runs, whose copy numbers start over, and a
//! folder that only ever receives one tree cannot add stale files to a paste.
//!
//! The push holds the wire, as an image push does. A newer Mac copy ends it at
//! its next reply; an agent restart starts it over. The guest's next CLIPXF
//! lists the files below `<clipdir>\files\`: that is this push coming back,
//! which clip_files.rs learns and never pulls.

use super::*;
use host_pasteboard::HostFileCopy;

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
    next_chunk: usize,
    phase: ClipImagePhase,
}

impl ClipFilesPush {
    pub(super) fn new(copy: u64, files: HostFileCopy, text: Option<String>) -> Self {
        let folder = u64::from_be_bytes(files.digest[..8].try_into().unwrap()) >> 1;
        let mut push = Self {
            copy,
            folder,
            files,
            text,
            entry: 0,
            next_chunk: 0,
            phase: ClipImagePhase::Beg,
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

    fn next_file(&self, from: usize) -> usize {
        let entries = &self.files.entries;
        (from..entries.len()).find(|index| entries[*index].data.is_some()).unwrap_or(entries.len())
    }

    fn data(&self) -> &[u8] {
        self.files.entries[self.entry].data.as_deref().unwrap_or_default()
    }

    fn chunks(&self, chunk: usize) -> usize {
        self.data().len().div_ceil(chunk)
    }
}

impl AgentConsoleHarness {
    /// A Finder copy within the limits: its files and its names in one push,
    /// after any older push still queued is dropped.
    pub(super) fn enqueue_host_files(&mut self, text: Option<String>, files: HostFileCopy, now: Instant) {
        let text = self.host_text_that_fits(text, now);
        self.drop_queued_host_pushes();
        let push = ClipFilesPush::new(self.last_synced.host_copy, files, text);
        self.queue.push_back(ServiceReq::ClipFilesPush(push));
    }

    /// The first wire line: PUTBEG of the first file, or CLIPSETFILES.
    pub(super) fn write_clip_files_push_line(&mut self, push: &ClipFilesPush) -> bool {
        let chunk = self.share_put_chunk_bytes();
        let Some(dir) = self.clip_images.as_ref().map(|images| images.dir.clone()) else {
            return false;
        };
        write_files_push_step(&mut self.service_line_scratch, &mut self.clip_crlf_scratch, &dir, push, chunk);
        true
    }

    /// PUTs, then CLIPSETFILES, one reply at a time.
    pub(super) fn handle_clip_files_push_reply(&mut self, line: &str, now: Instant, send: &mut dyn FnMut(&[u8])) {
        let (put_data, chunk) = (self.put_data, self.share_put_chunk_bytes());
        let newer = self.queue.iter().any(is_host_push);
        let dir = self.clip_images.as_ref().map(|images| images.dir.clone());
        let Some((ServiceReq::ClipFilesPush(push), sent_at)) = self.in_flight.as_mut() else {
            return;
        };
        let error = match push.phase {
            ClipImagePhase::Beg => "ERR PUTBEG",
            ClipImagePhase::Chunk => "ERR PUTCHUNK",
            ClipImagePhase::End => "ERR PUTEND",
            ClipImagePhase::Set => "ERR CLIPSETFILES",
        };
        let acked = match push.phase {
            ClipImagePhase::Beg => line == "OK PUTBEG",
            ClipImagePhase::Chunk => {
                line.strip_prefix("OK PUTCHUNK ").and_then(|seq| seq.parse::<usize>().ok())
                    == push.next_chunk.checked_sub(1)
            }
            ClipImagePhase::End => line.starts_with("PUTOK "),
            ClipImagePhase::Set => line.starts_with("OK CLIPSETFILES"),
        };
        if !acked {
            // An agent that lost the verb answers OUT; any other line is not ours.
            let set_refused = push.phase == ClipImagePhase::Set && line.starts_with("OUT ");
            if line.starts_with(error) || set_refused {
                println!("BVAGENT CLIPSYNC host->guest files-error {line}");
                self.last_synced.rebaseline = true;
                self.in_flight = None;
            }
            return;
        }
        let Some(dir) = dir.filter(|_| push.phase == ClipImagePhase::Set || !newer) else {
            // A newer Mac copy waits, or images went off: the guest clipboard is as it was.
            self.in_flight = None;
            return;
        };
        *sent_at = now;
        let out = &mut self.service_line_scratch;
        out.clear();
        match push.phase {
            ClipImagePhase::Beg | ClipImagePhase::Chunk if push.next_chunk < push.chunks(chunk) => {
                let start = push.next_chunk * chunk;
                let end = (start + chunk).min(push.data().len());
                put_chunk_line_into(out, put_data, push.next_chunk, &push.data()[start..end]);
                push.next_chunk += 1;
                push.phase = ClipImagePhase::Chunk;
            }
            ClipImagePhase::Beg | ClipImagePhase::Chunk => {
                let _ = writeln!(out, "PUTEND {}", push.chunks(chunk));
                push.phase = ClipImagePhase::End;
            }
            ClipImagePhase::End => {
                push.entry = push.next_file(push.entry + 1);
                push.next_chunk = 0;
                let done = push.entry == push.files.entries.len();
                push.phase = if done { ClipImagePhase::Set } else { ClipImagePhase::Beg };
                write_files_push_step(out, &mut self.clip_crlf_scratch, &dir, push, chunk);
            }
            ClipImagePhase::Set => {
                let Some((ServiceReq::ClipFilesPush(push), _)) = self.in_flight.take() else {
                    return;
                };
                self.last_synced.text = push.text.as_deref().map(normalize_clip);
                self.last_synced.image = None;
                self.last_synced.html = None;
                self.last_synced.files = None;
                self.last_synced.rebaseline = false;
                self.last_synced.echo = true;
                clip_line(format!(
                    "BVAGENT CLIPSYNC host->guest files={} bytes={} copy={} t={}",
                    push.files.entries.len(),
                    push.files.bytes,
                    push.copy,
                    self.t_ms(now)
                ));
                return;
            }
        }
        self.last_send = Some(now);
        send(self.service_line_scratch.as_bytes());
    }
}

/// A Mac push of any kind: text, image or files.
pub(super) fn is_host_push(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::ClipPush(..) | ServiceReq::ClipImagePush(_) | ServiceReq::ClipFilesPush(_))
}

/// PUTBEG of the push's current file, or CLIPSETFILES once every file is in.
fn write_files_push_step(out: &mut String, crlf: &mut String, dir: &str, push: &ClipFilesPush, chunk: usize) {
    if push.phase == ClipImagePhase::Set {
        let list: String = push
            .files
            .entries
            .iter()
            .map(|entry| format!("{}|{}\n", u8::from(entry.data.is_none()), entry.rel))
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
        return;
    }
    let path = format!("{dir}\\files\\{}\\{}", push.folder, push.files.entries[push.entry].rel);
    out.push_str("PUTBEG ");
    base64_encode_into(path.as_bytes(), out);
    let _ = writeln!(out, " {} {}", push.data().len(), push.chunks(chunk));
}
