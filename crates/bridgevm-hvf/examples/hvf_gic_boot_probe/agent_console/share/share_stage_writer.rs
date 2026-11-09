//! A large guest file reaches the Mac on a thread of its own. The service, on vCPU 0's thread,
//! only hands over each GETCHUNK's base64 text as it arrives; this thread decodes it, hashes
//! it, writes it into the destination's unpublished temporary and, once told to, seals, dates,
//! F_FULLFSYNCs and renames it (host_destination.rs). The service polls for the answers, so
//! neither the writes of a GB-sized file nor its final sync stop the guest's vCPU 0.
//!
//! Dropping the writer abandons the file: the thread removes its temporary and ends. With the
//! share's synced state on, the name is marked on disk right before the publish
//! (share_synced.rs), here, so the service thread writes nothing more for it.

use super::host_destination::{Destination, Staged};
use crate::agent_console::state::sync_file::{mark, SharedMarks};
use super::*;
use std::io::{self, Write as _};
use std::sync::mpsc;
use std::time::SystemTime;

enum ToWriter {
    Chunk(String),
    Publish(Option<SystemTime>),
}

pub(in crate::agent_console) enum FromWriter {
    /// Every byte of the file is written; its FNV-1a hash.
    Hashed(u64),
    /// The publish's result: the Mac file's modification time, or why it failed.
    Published(io::Result<Option<u128>>),
    /// A chunk could not be decoded or written; the temporary is gone.
    Failed(io::Error),
}

pub(in crate::agent_console) struct StageWriter {
    to: mpsc::Sender<ToWriter>,
    from: mpsc::Receiver<(u64, Option<FromWriter>)>,
    /// Bytes handed over and not yet written.
    pub(in crate::agent_console) backlog: u64,
}

impl StageWriter {
    /// Stage `size` bytes for `destination` (checking it can be replaced) and start the thread;
    /// `marks` and the name mark it on disk before the publish.
    pub(in crate::agent_console) fn start(
        destination: Destination,
        size: u64,
        marks: Option<(SharedMarks, String)>,
    ) -> io::Result<Self> {
        let staged = destination.stage()?;
        let (to, inbox) = mpsc::channel();
        let (outbox, from) = mpsc::channel();
        let spawned = std::thread::Builder::new()
            .name("bvagent-share-stage".into())
            .spawn(move || run(&destination, staged, size, marks, &inbox, &outbox));
        // A failed spawn drops the closure, and with it the staged file, unremoved.
        spawned?;
        Ok(Self {
            to,
            from,
            backlog: 0,
        })
    }

    /// One GETCHUNK payload, `bytes` once decoded, next in file order.
    pub(in crate::agent_console) fn chunk(&mut self, b64: String, bytes: u64) {
        self.backlog += bytes;
        let _ = self.to.send(ToWriter::Chunk(b64));
    }

    pub(in crate::agent_console) fn publish(&mut self, mtime: Option<SystemTime>) {
        let _ = self.to.send(ToWriter::Publish(mtime));
    }

    /// The thread's next answer, without waiting.
    pub(in crate::agent_console) fn poll(&mut self) -> Option<FromWriter> {
        loop {
            match self.from.try_recv() {
                Ok((written, answer)) => {
                    self.backlog = self.backlog.saturating_sub(written);
                    if answer.is_some() {
                        return answer;
                    }
                }
                Err(mpsc::TryRecvError::Empty) => return None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Some(FromWriter::Failed(io::Error::other("writer thread ended")))
                }
            }
        }
    }
}

fn run(
    destination: &Destination,
    mut staged: Staged,
    size: u64,
    marks: Option<(SharedMarks, String)>,
    inbox: &mpsc::Receiver<ToWriter>,
    outbox: &mpsc::Sender<(u64, Option<FromWriter>)>,
) {
    let mut hash = share_sync::FNV1A64_START;
    let mut written = 0u64;
    while let Ok(message) = inbox.recv() {
        match message {
            ToWriter::Chunk(b64) => {
                let bytes = match base64_decode(&b64) {
                    Ok(bytes) if written + bytes.len() as u64 <= size => bytes,
                    _ => {
                        let error = io::Error::new(io::ErrorKind::InvalidData, "bad chunk");
                        let _ = outbox.send((0, Some(FromWriter::Failed(error))));
                        break;
                    }
                };
                if let Err(error) = staged.file.write_all(&bytes) {
                    let _ = outbox.send((0, Some(FromWriter::Failed(error))));
                    break;
                }
                hash = share_sync::fnv1a64_continue(hash, &bytes);
                written += bytes.len() as u64;
                let answer = (written == size).then_some(FromWriter::Hashed(hash));
                let _ = outbox.send((bytes.len() as u64, answer));
            }
            ToWriter::Publish(mtime) => {
                if written != size {
                    break;
                }
                // A mark that cannot be written turns the state off (the service sees it).
                if let Some((marks, name)) = &marks {
                    let _ = mark(marks, [name.as_str()]);
                }
                let result = destination.publish(staged, mtime);
                let _ = outbox.send((0, Some(FromWriter::Published(result))));
                return;
            }
        }
    }
    destination.discard(staged);
}

/// The byte count of a base64 text, or None when it cannot be base64.
pub(in crate::agent_console) fn decoded_len(b64: &str) -> Option<u64> {
    let bytes = b64.as_bytes();
    if bytes.len() % 4 != 0 {
        return None;
    }
    let pad = bytes.iter().rev().take(2).take_while(|b| **b == b'=').count();
    Some((bytes.len() / 4 * 3 - pad) as u64)
}

#[cfg(test)]
#[path = "share_stage_writer_tests.rs"]
mod tests;
