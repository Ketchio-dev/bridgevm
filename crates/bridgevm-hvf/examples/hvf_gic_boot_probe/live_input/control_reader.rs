use super::{LiveInputController, COMPACT_AFTER_BYTES, MAX_PENDING_COMMANDS, MAX_READ_BYTES_PER_TICK};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};

impl LiveInputController {
    pub(super) fn read_new_commands_locked(&mut self, file: &mut File) {
        let Ok(len) = file.metadata().map(|metadata| metadata.len()) else {
            return;
        };
        if len < self.offset {
            self.offset = 0;
            self.lines.clear();
        }
        if len == self.offset {
            return;
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let unread = len.saturating_sub(self.offset);
        let read_limit = unread.min(MAX_READ_BYTES_PER_TICK);
        let mut bytes = Vec::with_capacity(read_limit as usize);
        if (&mut *file)
            .take(read_limit)
            .read_to_end(&mut bytes)
            .is_err()
        {
            return;
        }
        // Retain unread frames in the file, not in an unbounded second queue.
        // Each chunk completes at most one line, including CRLF/partial lines.
        for chunk in bytes.split_inclusive(|byte| *byte == b'\n') {
            if self.pending.len() >= MAX_PENDING_COMMANDS {
                break;
            }
            self.offset = self.offset.saturating_add(chunk.len() as u64);
            for line in self.lines.consume(chunk) {
                self.push_line(&line);
            }
        }
        if self.offset == len
            && self.offset >= COMPACT_AFTER_BYTES
            && self.lines.is_empty()
            && file.set_len(0).is_ok()
        {
            self.offset = 0;
        }
    }
}
