//! Assemble stereo-s16 DMA fragments before publishing them to the callback.
//! The ring always starts and ends on a complete four-byte frame. Loss rejects
//! whole frames, never shifting the phase of the next accepted source bytes.

use super::BYTES_PER_FRAME;
use std::collections::VecDeque;

const FRAME: usize = BYTES_PER_FRAME as usize;

#[derive(Default)]
pub(super) struct PcmFrames {
    tail: [u8; FRAME],
    len: usize,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Publication {
    pub(super) guest_frames: u64,
    pub(super) dropped_bytes: usize,
}

impl PcmFrames {
    /// One publication batch per sink write. A rejected batch contains only
    /// complete guest frames; its final fragment stays private for the next call.
    pub(super) fn write(
        &mut self,
        samples: &[u8],
        ring: &mut VecDeque<u8>,
        capacity: usize,
    ) -> Publication {
        debug_assert_eq!(ring.len() % FRAME, 0);
        let complete = (samples.len() + self.len) / FRAME * FRAME;
        if complete == 0 {
            self.tail[self.len..self.len + samples.len()].copy_from_slice(samples);
            self.len += samples.len();
            return Publication::default();
        }
        let consumed = complete - self.len;
        let result = if complete <= capacity.saturating_sub(ring.len()) {
            ring.extend(self.tail[..self.len].iter().copied());
            ring.extend(samples[..consumed].iter().copied());
            Publication {
                guest_frames: (complete / FRAME) as u64,
                dropped_bytes: 0,
            }
        } else {
            Publication {
                guest_frames: 0,
                dropped_bytes: complete,
            }
        };
        self.len = samples.len() - consumed;
        self.tail[..self.len].copy_from_slice(&samples[consumed..]);
        result
    }

    /// End a generation, not a RUN pause. Preserve supplied bytes with terminal
    /// zero padding; this constructed frame is NOT counted as a guest frame.
    pub(super) fn finish(&mut self, ring: &mut VecDeque<u8>, capacity: usize) -> Publication {
        debug_assert_eq!(ring.len() % FRAME, 0);
        let mut result = Publication::default();
        if self.len != 0 {
            if FRAME <= capacity.saturating_sub(ring.len()) {
                self.tail[self.len..].fill(0);
                ring.extend(self.tail);
            } else {
                result.dropped_bytes = self.len;
            }
            self.len = 0;
        }
        result
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_frame_tests.rs"]
mod tests;
