//! Finder drops. The app stages the dropped files (regular files and folders only, names already
//! cleaned for Windows) and appends `BVDROP <id> <b64(staging folder)>` to the control file; this
//! streams that tree into the guest user's Desktop. Jobs run one at a time in arrival order.
//!
//! Every wire step is its own lockstep request, so control and clipboard traffic still go out
//! between chunks, and a file is read one chunk at a time. The guest's reply wakes the service at
//! once (`AGENT_INBOUND_HOOK`, ctl_wake.rs), and the next step leaves in that same tick.

use super::*;
use std::collections::HashMap;
use std::io::Read as _;

/// Raw payload per DROPCHUNK line; base64 makes the line a third longer.
pub(super) const DROP_CHUNK_BYTES: usize = 1024 * 1024;
const DROP_PROGRESS_INTERVAL: Duration = Duration::from_millis(500);

/// One host->guest drop step. Each has a one-line reply; an agent without these verbs answers
/// `OUT 255` (unknown token) and writes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum DropReq {
    /// `DROPCAPS` -> `BVDROP_CAPS 1`, the capability marker.
    Caps,
    /// `DROPDIR <b64(rel)>` -> `OK DROPDIR <b64(final name)>`.
    Dir { rel: String },
    /// `DROPBEG <b64(rel)> <size>` -> `OK DROPBEG <b64(final name)>`.
    Beg { rel: String, size: u64 },
    /// `DROPCHUNK <seq> <chars>`, then the payload as its own line of `<chars>` base64
    /// characters (the guest reads it with a compiled reader) -> `OK DROPCHUNK <seq>`.
    Chunk { seq: u64, bytes: Vec<u8> },
    /// `DROPEND <chunks>` -> `OK DROPEND <bytes>`.
    End { chunks: u64 },
    /// `DROPABORT` -> `OK DROPABORT`: the guest deletes its half-written file.
    Abort,
}

pub(super) fn is_drop_req(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::Drop(_))
}

struct DropEntry {
    /// Path below the staging folder, '/'-separated.
    host_rel: String,
    /// None for a folder.
    size: Option<u64>,
}

struct DropFile {
    file: std::fs::File,
    size: u64,
    read: u64,
    chunks: u64,
    begun: bool,
}

struct ActiveDrop {
    id: String,
    root: PathBuf,
    entries: Vec<DropEntry>,
    next: usize,
    files: u64,
    bytes: u64,
    done_files: u64,
    done_bytes: u64,
    /// Host folder -> the guest folder it became (the guest renames on a name collision).
    folders: HashMap<String, String>,
    file: Option<DropFile>,
    caps: bool,
    started: Instant,
    last_progress: Option<Instant>,
    /// Chunk round trips (send to reply) for the done line's per-chunk timing.
    chunks: u64,
    chunk_wire: Duration,
}

#[derive(Default)]
pub(super) struct FileDrops {
    waiting: VecDeque<(String, PathBuf)>,
    active: Option<ActiveDrop>,
}

impl FileDrops {
    /// `<id> <b64(staging folder)>` from a `BVDROP` control line.
    pub(super) fn submit(&mut self, request: &str) {
        let parsed = request.split_once(' ').and_then(|(id, b64)| {
            let id_ok = !id.is_empty()
                && id.len() <= 64
                && id.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-');
            let path = String::from_utf8(base64_decode(b64.trim()).ok()?).ok()?;
            (id_ok && path.starts_with('/')).then(|| (id.to_string(), PathBuf::from(path)))
        });
        match parsed {
            Some(job) => self.waiting.push_back(job),
            None => println!("BVAGENT DROP failed - bad-request"),
        }
    }
}

/// Folders before their contents, names sorted. Only regular files and folders count: a symlink,
/// a device or a name Windows would split at a backslash is skipped.
fn walk_staged(root: &Path, rel: &str, out: &mut Vec<DropEntry>) -> std::io::Result<()> {
    let mut names = Vec::new();
    for entry in std::fs::read_dir(root.join(rel))? {
        if let Ok(name) = entry?.file_name().into_string() {
            if !name.contains('\\') {
                names.push(name);
            }
        }
    }
    names.sort();
    for name in names {
        let host_rel = if rel.is_empty() {
            name
        } else {
            format!("{rel}/{name}")
        };
        let meta = std::fs::symlink_metadata(root.join(&host_rel))?;
        if meta.is_dir() {
            out.push(DropEntry {
                host_rel: host_rel.clone(),
                size: None,
            });
            walk_staged(root, &host_rel, out)?;
        } else if meta.is_file() {
            out.push(DropEntry {
                host_rel,
                size: Some(meta.len()),
            });
        }
    }
    Ok(())
}

/// The guest path (below the Desktop) for a host entry, inside its parent's final guest name.
fn guest_rel(folders: &HashMap<String, String>, host_rel: &str) -> String {
    match host_rel.rsplit_once('/') {
        Some((parent, leaf)) => {
            debug_assert!(folders.contains_key(parent), "folders precede their contents");
            let parent = folders.get(parent).map_or(parent, String::as_str);
            format!("{parent}\\{leaf}")
        }
        None => host_rel.to_string(),
    }
}

impl ActiveDrop {
    fn next_req(&mut self) -> Result<Option<DropReq>, &'static str> {
        if !self.caps {
            return Ok(Some(DropReq::Caps));
        }
        if let Some(file) = self.file.as_mut() {
            debug_assert!(file.begun, "chunks follow the guest's OK DROPBEG");
            if file.read < file.size {
                let len = (file.size - file.read).min(DROP_CHUNK_BYTES as u64) as usize;
                let mut bytes = vec![0; len];
                file.file.read_exact(&mut bytes).map_err(|_| "read-error")?;
                file.read += len as u64;
                file.chunks += 1;
                return Ok(Some(DropReq::Chunk {
                    seq: file.chunks - 1,
                    bytes,
                }));
            }
            return Ok(Some(DropReq::End {
                chunks: file.chunks,
            }));
        }
        let Some(entry) = self.entries.get(self.next) else {
            return Ok(None);
        };
        let rel = guest_rel(&self.folders, &entry.host_rel);
        Ok(Some(match entry.size {
            None => DropReq::Dir { rel },
            Some(size) => {
                let file = std::fs::File::open(self.root.join(&entry.host_rel))
                    .map_err(|_| "read-error")?;
                self.file = Some(DropFile {
                    file,
                    size,
                    read: 0,
                    chunks: 0,
                    begun: false,
                });
                DropReq::Beg { rel, size }
            }
        }))
    }
}

enum DropReply {
    Ignored,
    Failed(String),
    Caps,
    Named(String),
    Chunk(u64),
    End(u64),
    Aborted,
}

/// A guest name (one path component) the next requests may build on.
fn guest_name(b64: &str) -> Option<String> {
    let name = String::from_utf8(base64_decode(b64).ok()?).ok()?;
    let ok = !name.is_empty() && name != "." && name != ".." && !name.contains(['\\', '/']);
    ok.then_some(name)
}

fn classify_drop_reply(req: &DropReq, line: &str) -> DropReply {
    if parse_out_line(line).is_some_and(|(exit, _)| exit == 255) {
        return DropReply::Failed("agent-too-old".into());
    }
    let err = match req {
        DropReq::Caps => "ERR DROPCAPS",
        DropReq::Dir { .. } => "ERR DROPDIR",
        DropReq::Beg { .. } => "ERR DROPBEG",
        DropReq::Chunk { .. } => "ERR DROPCHUNK",
        DropReq::End { .. } => "ERR DROPEND",
        DropReq::Abort => "ERR DROPABORT",
    };
    if let Some(rest) = line.strip_prefix(err) {
        if rest.is_empty() || rest.starts_with(' ') {
            let detail: String = rest.trim().chars().take(120).collect();
            return DropReply::Failed(format!("guest-error {detail}").trim_end().into());
        }
    }
    match req {
        DropReq::Caps if line.starts_with("BVDROP_CAPS ") => DropReply::Caps,
        DropReq::Dir { .. } | DropReq::Beg { .. } => {
            let ok = if matches!(req, DropReq::Dir { .. }) {
                "OK DROPDIR "
            } else {
                "OK DROPBEG "
            };
            match line.strip_prefix(ok) {
                Some(b64) => guest_name(b64).map_or_else(
                    || DropReply::Failed("guest-error bad-name".into()),
                    DropReply::Named,
                ),
                None => DropReply::Ignored,
            }
        }
        DropReq::Chunk { seq, bytes } if line == format!("OK DROPCHUNK {seq}") => {
            DropReply::Chunk(bytes.len() as u64)
        }
        DropReq::End { .. } => match line.strip_prefix("OK DROPEND ") {
            Some(n) => n
                .parse()
                .map_or(DropReply::Failed("guest-error".into()), DropReply::End),
            None => DropReply::Ignored,
        },
        DropReq::Abort if line == "OK DROPABORT" => DropReply::Aborted,
        _ => DropReply::Ignored,
    }
}

impl AgentConsoleHarness {
    /// Queue the active drop's next wire step once the previous one has its reply.
    pub(super) fn enqueue_file_drop(&mut self, now: Instant) {
        if self.any_pending(is_drop_req) {
            return;
        }
        if self.drops.active.is_none() && !self.start_next_drop(now) {
            return;
        }
        let Some(active) = self.drops.active.as_mut() else {
            return;
        };
        match active.next_req() {
            Ok(Some(req)) => self.queue.push_back(ServiceReq::Drop(req)),
            Ok(None) => {
                self.finish_drop(now);
                self.enqueue_file_drop(now);
            }
            Err(reason) => self.fail_drop(reason, now),
        }
    }

    fn start_next_drop(&mut self, now: Instant) -> bool {
        while let Some((id, root)) = self.drops.waiting.pop_front() {
            let mut entries = Vec::new();
            if walk_staged(&root, "", &mut entries).is_err() {
                println!("BVAGENT DROP failed {id} staging-unreadable t={}", self.t_ms(now));
                continue;
            }
            let files = entries.iter().filter(|e| e.size.is_some()).count() as u64;
            let bytes = entries.iter().filter_map(|e| e.size).sum();
            println!(
                "BVAGENT DROP start {id} files={files} bytes={bytes} t={}",
                self.t_ms(now)
            );
            self.drops.active = Some(ActiveDrop {
                id,
                root,
                entries,
                next: 0,
                files,
                bytes,
                done_files: 0,
                done_bytes: 0,
                folders: HashMap::new(),
                file: None,
                caps: false,
                started: now,
                last_progress: None,
                chunks: 0,
                chunk_wire: Duration::ZERO,
            });
            return true;
        }
        false
    }

    fn finish_drop(&mut self, now: Instant) {
        let t = self.t_ms(now);
        let Some(active) = self.drops.active.take() else {
            return;
        };
        let chunk_us = active
            .chunk_wire
            .as_micros()
            .checked_div(u128::from(active.chunks))
            .unwrap_or(0);
        println!(
            "BVAGENT DROP done {} files={} bytes={} ms={} chunks={} chunk_avg_us={chunk_us} t={t}",
            active.id,
            active.done_files,
            active.done_bytes,
            now.duration_since(active.started).as_millis(),
            active.chunks
        );
    }

    /// End the active drop. A host-side failure mid-file also asks the guest to delete it.
    pub(super) fn fail_drop(&mut self, reason: &str, now: Instant) {
        let t = self.t_ms(now);
        let Some(active) = self.drops.active.take() else {
            return;
        };
        println!(
            "BVAGENT DROP failed {} {reason} files={}/{} bytes={}/{} t={t}",
            active.id, active.done_files, active.files, active.done_bytes, active.bytes
        );
        let host_side = matches!(reason, "read-error" | "stalled");
        if host_side && active.file.as_ref().is_some_and(|file| file.begun) {
            self.queue.push_back(ServiceReq::Drop(DropReq::Abort));
        }
    }

    pub(super) fn handle_drop_reply(&mut self, line: &str, now: Instant) {
        let (reply, sent_at) = match self.in_flight.as_ref() {
            Some((ServiceReq::Drop(req), sent_at)) => (classify_drop_reply(req, line), *sent_at),
            _ => return,
        };
        if matches!(reply, DropReply::Ignored) {
            return;
        }
        self.complete_in_flight();
        let Some(active) = self.drops.active.as_mut() else {
            return;
        };
        match reply {
            DropReply::Ignored | DropReply::Aborted => {}
            DropReply::Failed(reason) => return self.fail_drop(&reason, now),
            DropReply::Caps => active.caps = true,
            DropReply::Named(name) => match active.file.as_mut() {
                Some(file) => file.begun = true,
                None => {
                    let host_rel = active.entries[active.next].host_rel.clone();
                    let guest = match host_rel.rsplit_once('/') {
                        Some((parent, _)) => {
                            let parent = active.folders.get(parent).map_or(parent, String::as_str);
                            format!("{parent}\\{name}")
                        }
                        None => name,
                    };
                    active.folders.insert(host_rel, guest);
                    active.next += 1;
                }
            },
            DropReply::Chunk(len) => {
                active.done_bytes += len;
                active.chunks += 1;
                active.chunk_wire += now.saturating_duration_since(sent_at);
            }
            DropReply::End(written) => {
                if active.file.as_ref().map(|file| file.size) != Some(written) {
                    return self.fail_drop("guest-error short", now);
                }
                active.file = None;
                active.done_files += 1;
                active.next += 1;
            }
        }
        let due = active
            .last_progress
            .is_none_or(|last| now.duration_since(last) >= DROP_PROGRESS_INTERVAL);
        if due {
            active.last_progress = Some(now);
            println!(
                "BVAGENT DROP progress {} files={}/{} bytes={}/{} t={}",
                active.id,
                active.done_files,
                active.files,
                active.done_bytes,
                active.bytes,
                now.duration_since(self.start).as_millis()
            );
        }
    }
}

pub(super) fn write_drop_wire_line(req: &DropReq, out: &mut String) {
    match req {
        DropReq::Caps => out.push_str("DROPCAPS"),
        DropReq::Dir { rel } => {
            out.push_str("DROPDIR ");
            base64_encode_into(rel.as_bytes(), out);
        }
        DropReq::Beg { rel, size } => {
            out.push_str("DROPBEG ");
            base64_encode_into(rel.as_bytes(), out);
            let _ = write!(out, " {size}");
        }
        DropReq::Chunk { seq, bytes } => {
            let _ = writeln!(out, "DROPCHUNK {seq} {}", bytes.len().div_ceil(3) * 4);
            base64_encode_into(bytes, out);
        }
        DropReq::End { chunks } => {
            let _ = write!(out, "DROPEND {chunks}");
        }
        DropReq::Abort => out.push_str("DROPABORT"),
    }
    out.push('\n');
}

#[cfg(test)]
#[path = "file_drop_tests.rs"]
mod tests;
