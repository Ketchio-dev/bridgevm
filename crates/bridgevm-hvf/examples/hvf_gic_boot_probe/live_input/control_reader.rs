use super::restart_resume::AT_END;
use super::{
    LiveInputCommand, LiveInputController, COMPACT_AFTER_BYTES, MAX_PENDING_COMMANDS,
    MAX_READ_BYTES_PER_TICK,
};
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
#[path = "read_budget.rs"]
mod read_budget;
impl LiveInputController {
    pub(super) fn read_new_commands_locked(&mut self, file: &mut File) {
        let Ok(len) = file.metadata().map(|metadata| metadata.len()) else {
            return;
        };
        if self.offset == AT_END {
            self.offset = len;
        }
        if len < self.offset {
            self.offset = 0;
            self.lines.clear();
            self.power_scanned_to = 0;
        }
        if len == self.offset {
            return;
        }
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return;
        }
        let read_limit = read_budget::byte_budget(self.pending.len(), len.saturating_sub(self.offset));
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
                // The read-ahead below already queued this one.
                if self.offset <= self.power_scanned_to
                    && LiveInputCommand::host_control(line.as_bytes()).is_some()
                {
                    continue;
                }
                self.push_line(&line);
            }
        }
        if self.pending.len() >= MAX_PENDING_COMMANDS {
            self.read_ahead_for_host_control(file, len);
        }
        if self.offset == len
            && self.offset >= COMPACT_AFTER_BYTES
            && self.lines.is_empty()
            && file.set_len(0).is_ok()
        {
            self.offset = 0;
            self.power_scanned_to = 0;
        }
    }

    /// A full queue leaves unread lines in the file while the guest is not
    /// reading HID (early boot, a stalled USB stack, a hung guest). Queue the
    /// POWERBUTTON and RESET lines among them anyway (`apply_pending` presses
    /// the button first, the run loop takes a reset); the reader skips them
    /// when it reaches them.
    fn read_ahead_for_host_control(&mut self, file: &mut File, len: u64) {
        const MAX_QUEUED_PRESSES: usize = 4;
        if self.power_scanned_to <= self.offset && !self.lines.is_empty() {
            return; // Mid-line: read ahead from the next line boundary only.
        }
        let start = self.offset.max(self.power_scanned_to);
        let mut bytes = Vec::new();
        if start >= len
            || file.seek(SeekFrom::Start(start)).is_err()
            || (&mut *file)
                .take(MAX_READ_BYTES_PER_TICK.min(len - start))
                .read_to_end(&mut bytes)
                .is_err()
        {
            return;
        }
        let mut scanned = start;
        let mut queued = 0;
        for line in bytes.split_inclusive(|byte| *byte == b'\n') {
            if !line.ends_with(b"\n") || queued == MAX_QUEUED_PRESSES {
                break;
            }
            scanned += line.len() as u64;
            if let Some(command) = LiveInputCommand::host_control(line.trim_ascii()) {
                self.pending.push_back(command);
                queued += 1;
            }
        }
        self.power_scanned_to = scanned;
    }
}
