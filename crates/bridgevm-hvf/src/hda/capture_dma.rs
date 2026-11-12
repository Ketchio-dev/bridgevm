//! The capture stream (SD0): DMA from the host capture source into guest memory.
//!
//! Paced like playback: each poll moves the bytes that the stream's format
//! covers in the elapsed host time, capped at `MAX_POLL_ELAPSED`. The source
//! fills each chunk and silence fills whatever it cannot, so the guest always
//! receives a continuous stream. The chunk is written at the buffer descriptor's
//! address before LPIB and the DMA position entry advance past it, so a position
//! the guest reads never runs ahead of the data. Descriptor and address checks
//! are playback's: a fault halts the stream with DESE.

use super::*;

/// The time to play or record for a poll at `now`, moving the stream's time
/// base to `now`; `None` for the poll that sets the base. Two threads poll
/// (a vCPU drain and the audio tick), each with a time it took before it got
/// the platform, so `now` can be older than the base. The base never moves
/// back: it would make the next poll count the same interval again.
pub(super) fn elapsed_since(last_poll: &mut Option<Instant>, now: Instant) -> Option<Duration> {
    let last = last_poll.replace(last_poll.map_or(now, |last| last.max(now)))?;
    Some(now.saturating_duration_since(last).min(MAX_POLL_ELAPSED))
}

impl HdaController {
    pub(crate) fn poll_capture(&mut self, mem: &mut dyn GuestMemoryMut, now: Option<Instant>) {
        if self.capture.ctl & SDCTL_RUN == 0 || self.gctl & GCTL_CRST == 0 {
            self.capture_last_poll = now;
            return;
        }
        let elapsed = match now {
            Some(now) => match elapsed_since(&mut self.capture_last_poll, now) {
                Some(elapsed) => elapsed,
                None => return,
            },
            None => TEST_POLL_QUANTUM,
        };
        self.poll_capture_for_duration(mem, elapsed);
    }

    pub fn poll_capture_for_duration(&mut self, mem: &mut dyn GuestMemoryMut, elapsed: Duration) {
        if self.capture.ctl & SDCTL_RUN == 0 || self.gctl & GCTL_CRST == 0 {
            return;
        }
        let Some(bytes_per_second) = stream_bytes_per_second(self.capture.fmt) else {
            self.capture.sts |= SDSTS_DESE;
            return;
        };
        let numerator = elapsed.as_nanos() * u128::from(bytes_per_second)
            + u128::from(self.capture_byte_time_remainder);
        let whole_bytes = numerator / 1_000_000_000;
        let frame_bytes = usize::from(stream_frame_bytes(self.capture.fmt).unwrap_or(1));
        let budget = (whole_bytes as usize / frame_bytes) * frame_bytes;
        self.capture_byte_time_remainder =
            (numerator % 1_000_000_000 + (whole_bytes - budget as u128) * 1_000_000_000) as u64;
        self.fill_capture(mem, budget);
    }

    fn fill_capture(&mut self, mem: &mut dyn GuestMemoryMut, mut budget: usize) {
        if budget == 0 || self.capture.cbl == 0 || self.capture.bdl == 0 {
            return;
        }
        let Some((rate, channels, bits)) = stream_pcm_format(self.capture.fmt) else {
            self.capture.sts |= SDSTS_DESE;
            return;
        };
        while budget > 0 && self.capture.ctl & SDCTL_RUN != 0 {
            let offset = u64::from(self.capture.bdl_index) * 16;
            let mut raw = [0u8; 16];
            let read = self
                .capture
                .bdl
                .checked_add(offset)
                .is_some_and(|address| mem.read_into(address, &mut raw));
            if !read {
                self.halt_capture_dma();
                break;
            }
            let address = u64::from_le_bytes(raw[..8].try_into().unwrap());
            let length = u32::from_le_bytes(raw[8..12].try_into().unwrap());
            let flags = u32::from_le_bytes(raw[12..16].try_into().unwrap());
            if length == 0 {
                self.halt_capture_dma();
                break;
            }
            if self.capture.bdl_offset >= length {
                self.complete_capture_entry(flags);
                continue;
            }
            let remaining_entry = (length - self.capture.bdl_offset) as usize;
            let remaining_cbl =
                (self.capture.cbl - self.capture.lpib.min(self.capture.cbl)) as usize;
            let chunk_len = budget.min(remaining_entry).min(remaining_cbl);
            if chunk_len == 0 {
                self.wrap_capture_buffer();
                self.write_capture_position(mem);
                continue;
            }
            self.capture_scratch.clear();
            self.capture_scratch.resize(chunk_len, 0);
            if let Some(source) = self.pcm_source.as_mut() {
                let filled = source.read_pcm(&mut self.capture_scratch, rate, channels, bits);
                self.capture_scratch[filled.min(chunk_len)..].fill(0);
            }
            let dma_address = address.checked_add(u64::from(self.capture.bdl_offset));
            if !dma_address.is_some_and(|address| mem.write_bytes(address, &self.capture_scratch)) {
                self.halt_capture_dma();
                break;
            }
            self.capture.bdl_offset += chunk_len as u32;
            self.capture.lpib += chunk_len as u32;
            budget -= chunk_len;
            self.write_capture_position(mem);
            if self.capture.bdl_offset == length {
                self.complete_capture_entry(flags);
            }
            if self.capture.lpib >= self.capture.cbl {
                self.wrap_capture_buffer();
                self.write_capture_position(mem);
            }
        }
    }

    fn complete_capture_entry(&mut self, flags: u32) {
        if flags & BDL_IOC != 0 {
            self.capture.sts |= SDSTS_BCIS;
        }
        self.capture.bdl_offset = 0;
        self.capture.bdl_index = if self.capture.bdl_index >= self.capture.lvi {
            0
        } else {
            self.capture.bdl_index + 1
        };
    }

    fn wrap_capture_buffer(&mut self) {
        self.capture.lpib = 0;
        self.capture.bdl_index = 0;
        self.capture.bdl_offset = 0;
    }

    fn write_capture_position(&self, mem: &mut dyn GuestMemoryMut) {
        if self.dp_base & 1 == 0 {
            return;
        }
        let entry = (self.dp_base & !0x7f).wrapping_add(CAPTURE_POSITION_ENTRY);
        let _ = mem.write_bytes(entry, &self.capture.lpib.to_le_bytes());
    }

    fn halt_capture_dma(&mut self) {
        self.capture.sts |= SDSTS_DESE;
        self.capture.ctl &= !SDCTL_RUN;
        self.sync_stream_activity();
        self.notify_capture_stopped(true);
    }

    /// SD0CTL: SRST keeps the programmed buffer and format, as playback does.
    pub(crate) fn write_capture_ctl(&mut self, next: u32) {
        let was_running = self.capture.ctl & SDCTL_RUN != 0;
        if next & SDCTL_SRST != 0 {
            let StreamDescriptor {
                fmt, bdl, cbl, lvi, ..
            } = self.capture;
            self.capture = StreamDescriptor {
                ctl: SDCTL_SRST,
                fmt,
                bdl,
                cbl,
                lvi,
                ..StreamDescriptor::default()
            };
        } else {
            self.capture.ctl = next;
        }
        let running = self.capture.ctl & SDCTL_RUN != 0;
        if running != was_running {
            self.capture_last_poll = None;
            self.capture_byte_time_remainder = 0;
        }
        self.sync_stream_activity();
        match (was_running, running) {
            (false, true) => {
                if let Some(source) = self.pcm_source.as_mut() {
                    source.stream_started();
                }
            }
            (true, false) => self.notify_capture_stopped(true),
            _ => {}
        }
    }

    pub(crate) fn notify_capture_stopped(&mut self, was_running: bool) {
        if let (true, Some(source)) = (was_running, self.pcm_source.as_mut()) {
            source.stream_stopped();
        }
    }

    pub(crate) fn capture_interrupt_pending(&self) -> bool {
        let (sts, ctl) = (self.capture.sts, self.capture.ctl);
        (sts & SDSTS_BCIS != 0 && ctl & SDCTL_IOCE != 0)
            || (sts & SDSTS_FIFOE != 0 && ctl & SDCTL_FEIE != 0)
            || (sts & SDSTS_DESE != 0 && ctl & SDCTL_DEIE != 0)
    }

    /// Install or clear the host capture source. Silence without one.
    pub fn set_pcm_source(&mut self, source: Option<Box<dyn HdaPcmSource>>) {
        self.pcm_source = source;
    }
}
