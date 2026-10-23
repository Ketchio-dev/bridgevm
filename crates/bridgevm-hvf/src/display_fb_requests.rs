//! Frame requests in the BVFB header: how a reader asks a writer for a frame.
//!
//! The header stays version 1. Bytes 32..64 were reserved and are never
//! written by `FbSink`, which writes only the 24 header bytes, the sequence at
//! byte 24 and the pixels, so readers that ignore them are unaffected:
//!
//! - bytes 32..40, the request: a little-endian u64 written only by the reader,
//!   with `pwrite`, once per display tick. It is the reader's
//!   `CLOCK_UPTIME_RAW` time in nanoseconds, so a writer on the same host can
//!   tell both that a new request came and how long ago it was posted.
//! - bytes 40..44, the marker: [`FRAME_REQUEST_MARKER`] as a little-endian
//!   u32, set only by a writer that answers requests. A reader posts nothing
//!   until it sees it, so an older writer keeps its own cadence.
//!
//! The ramfb exporter answers requests (display-export-20261023, F1 B2). The
//! virtio-gpu device does not; a marker left in a file it reuses only makes
//! the reader post requests that nobody reads.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use super::DisplayFramebuffer;

/// Byte offset of the reader's request word.
pub const FRAME_REQUEST_OFFSET: usize = 32;
/// Byte offset of the writer's marker word.
pub const FRAME_REQUEST_MARKER_OFFSET: usize = 40;
/// "BREQ" in file byte order.
pub const FRAME_REQUEST_MARKER: u32 = 0x5145_5242;

impl DisplayFramebuffer {
    /// Mark the mapped file as one whose writer answers frame requests; false
    /// before the first publish has mapped it.
    pub fn accept_frame_requests(&mut self) -> bool {
        if self.sink.map.is_null() {
            return false;
        }
        // SAFETY: the mapping is page-aligned and at least the 64-byte header.
        let marker =
            unsafe { &*(self.sink.map.add(FRAME_REQUEST_MARKER_OFFSET) as *const AtomicU32) };
        marker.store(FRAME_REQUEST_MARKER.to_le(), Ordering::Release);
        true
    }

    /// The reader's latest request word, 0 when it never posted one; `None`
    /// before the first publish has mapped the file.
    pub fn frame_request(&self) -> Option<u64> {
        if self.sink.map.is_null() {
            return None;
        }
        // SAFETY: as in `accept_frame_requests`; the word is 8-byte aligned.
        let request = unsafe { &*(self.sink.map.add(FRAME_REQUEST_OFFSET) as *const AtomicU64) };
        Some(u64::from_le(request.load(Ordering::Acquire)))
    }
}

#[cfg(test)]
#[path = "display_fb_requests_tests.rs"]
mod tests;
