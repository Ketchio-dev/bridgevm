//! Harness-mode GETEND. It completes only the GET command whose path its GETBEG
//! named, and it writes the file to BRIDGEVM_VIRTIO_CONSOLE_GET_DIR through the
//! share's pinned, no-follow writer, as guest-to-host share files are written,
//! creating missing levels of that directory as guest directories are.
//!
//! The directory is the operator's BRIDGEVM_VIRTIO_CONSOLE_GET_DIR; its deepest
//! existing ancestor is opened as configured, symlinks included. Each missing
//! level below it is created like a directory for a guest path (owner-only 0700
//! and marked, guest_write_directory.rs), not with create_dir_all's 0755, and
//! the file gets the share's file policy through Destination.

use super::host_destination::Destination;
use super::*;
use std::io;

impl AgentConsoleHarness {
    /// GETEND <seq> for the command labelled `label`. A GETEND for no GET that
    /// GETBEG began is unmatched, like any stray line, and the command stays in
    /// flight.
    pub(in crate::agent_console) fn end_get(&mut self, label: &str, rest: &str) -> ReplyProgress {
        if !self.get_began_for(label) {
            return ReplyProgress::Ignored;
        }
        self.finish_get(label, rest);
        ReplyProgress::Complete
    }
    /// Whether the GET being reassembled is the one GETBEG began for `command`:
    /// a raw `GET <b64(path)>` whose path that GETBEG named. The agent decodes
    /// the argument with .NET's FromBase64String, which skips whitespace.
    fn get_began_for(&self, command: &str) -> bool {
        let Some(accum) = self.get_accum.as_ref() else {
            return false;
        };
        let Some(argument) = command.strip_prefix("GET ") else {
            return false;
        };
        let argument: String = argument.split_ascii_whitespace().collect();
        base64_decode(&argument).is_ok_and(|path| String::from_utf8_lossy(&path) == accum.path)
    }
    /// Finalize: verify byte count, print a summary labelled by the requesting
    /// command, and if BRIDGEVM_VIRTIO_CONSOLE_GET_DIR is set, write the file
    /// there under the last component of the guest path. That name is opened
    /// relative to the directory without following a symlink, and a temporary
    /// file renamed over it replaces only a writable regular file, never
    /// writing into the existing one.
    fn finish_get(&mut self, label: &str, _rest: &str) {
        let Some(accum) = self.take_finished_get() else {
            return;
        };
        let ok = accum.bytes.len() == accum.total;
        let preview_len = accum.bytes.len().min(48);
        let mut head = String::with_capacity(preview_len * 2);
        for byte in &accum.bytes[..preview_len] {
            let _ = write!(&mut head, "{byte:02x}");
        }
        let mut written_note = String::new();
        if let Ok(dir) = std::env::var("BRIDGEVM_VIRTIO_CONSOLE_GET_DIR") {
            let name = accum
                .path
                .rsplit(['\\', '/'])
                .next()
                .filter(|s| !s.is_empty())
                .unwrap_or("bvagent-get.bin");
            let dir = Path::new(&dir);
            match get_dir_destination(dir, name).and_then(|d| d.write(&accum.bytes)) {
                Ok(_) => written_note = format!(" wrote={}", dir.join(name).display()),
                Err(e) => written_note = format!(" write-error={e}"),
            }
        }
        println!(
            "BVAGENT GET {label} path={} bytes={} expected={} chunks={}/{} ok={ok}{written_note} head={head}",
            accum.path, accum.bytes.len(), accum.total, accum.chunks_seen, accum.nchunks
        );
    }
}

/// The destination of the file `name` in the GET directory `dir`.
fn get_dir_destination(dir: &Path, name: &str) -> io::Result<Destination> {
    let mut existing = dir;
    let mut key = vec![name];
    while !existing.as_os_str().is_empty() && !existing.try_exists()? {
        let level = existing.file_name().and_then(|level| level.to_str());
        key.push(level.ok_or(io::ErrorKind::InvalidInput)?);
        existing = existing.parent().unwrap_or(Path::new(""));
    }
    key.reverse();
    let existing = if existing.as_os_str().is_empty() {
        Path::new(".")
    } else {
        existing
    };
    Destination::open(existing, &key.join("/"), true)
}
