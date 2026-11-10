//! The BVFB header words a frame writer stores atomically: the seqlock
//! sequence at byte 24 and the publish time at byte 48.
//!
//! The publish time is the writer's `CLOCK_UPTIME_RAW` time in nanoseconds
//! when a frame's bytes were complete: a little-endian u64 stored after the
//! pixels and before the closing even sequence, so a reader that takes the
//! frame under the seqlock takes its time with it. The app's display link
//! (`CACurrentMediaTime`) and its frame requests use the same clock, so the
//! app can tell a frame’s age at its target vsync (the `app display
//! pacing:` record). Readers from before the word ignore bytes 48..56, and a
//! writer from before it leaves them 0, which readers take as unknown. Every
//! writer of the file stamps its frames, so a time left by an earlier writer
//! never labels another writer's frame. Only the host reads it; the guest
//! never sees the file.
//!
//! The damage words say which pixels the frame changed against the file's
//! previous frame (the one at sequence - 2): bytes 56..64 hold x, y, width and
//! height as little-endian u16, and bytes 44..48 the low 32 bits of the even
//! sequence they belong to. Both are written inside the seqlock, by every
//! writer of the file (`write_inner`): a damaged publish stores its clipped
//! rectangle (0x0 when nothing changed), a full one the whole frame. A reader
//! trusts the rectangle only when the tag is the frame's own sequence and it
//! showed the frame just before (sequence - 2); bytes an older writer left
//! carry another tag. A frame too large for u16 stores an odd tag, which no
//! frame has, so readers copy it whole.

use std::sync::atomic::{AtomicU64, Ordering};

use super::{FbSink, Rect};

/// Byte offset of the publish-time word.
pub(crate) const PUBLISH_TIME_OFFSET: usize = 48;
// An aligned word after the frame-request words (32..44) and before the pixels.
const _: () = assert!(
    PUBLISH_TIME_OFFSET >= crate::display_fb::requests::FRAME_REQUEST_MARKER_OFFSET + 4
        && PUBLISH_TIME_OFFSET % 8 == 0
        && PUBLISH_TIME_OFFSET + 8 <= 64
);
/// Byte offset of the damage tag: the low 32 bits of the sequence the damage belongs to.
pub(crate) const DAMAGE_TAG_OFFSET: usize = 44;
/// Byte offset of the damage rectangle: x, y, width, height as u16.
pub(crate) const DAMAGE_RECT_OFFSET: usize = 56;
// The tag sits after the frame-request marker (40..44), the rectangle after the publish time.
const _: () = assert!(
    DAMAGE_TAG_OFFSET == crate::display_fb::requests::FRAME_REQUEST_MARKER_OFFSET + 4
        && DAMAGE_TAG_OFFSET + 4 <= PUBLISH_TIME_OFFSET
        && DAMAGE_RECT_OFFSET == PUBLISH_TIME_OFFSET + 8
        && DAMAGE_RECT_OFFSET + 8 <= 64
);

impl FbSink {
    pub(super) fn sequence(&self) -> &AtomicU64 {
        unsafe { &*(self.map.add(24) as *const AtomicU64) }
    }

    /// Store the frame's publish time: inside the seqlock, after its pixels.
    pub(super) fn stamp_publish_time(&self) {
        // SAFETY: the mapping is page-aligned and at least the 64-byte header.
        let word = unsafe { &*(self.map.add(PUBLISH_TIME_OFFSET) as *const AtomicU64) };
        word.store(uptime_ns().to_le(), Ordering::Relaxed);
    }

    /// Store what the frame being written changed. `odd` is the frame's open sequence; the frame
    /// publishes `odd + 1`.
    pub(super) fn stamp_damage(&self, odd: u64, changed: Rect) {
        let even = odd.wrapping_add(1) as u32;
        let fields = [changed.x, changed.y, changed.width, changed.height].map(u16::try_from);
        let (tag, rect) = match fields {
            [Ok(x), Ok(y), Ok(width), Ok(height)] => (even, [x, y, width, height]),
            _ => (even | 1, [0; 4]),
        };
        let mut words = [0u8; 8];
        for (slot, value) in words.chunks_exact_mut(2).zip(rect) {
            slot.copy_from_slice(&value.to_le_bytes());
        }
        // SAFETY: the mapping is page-aligned and at least the 64-byte header.
        unsafe {
            std::ptr::copy_nonoverlapping(
                tag.to_le_bytes().as_ptr(),
                self.map.add(DAMAGE_TAG_OFFSET),
                4,
            );
            std::ptr::copy_nonoverlapping(words.as_ptr(), self.map.add(DAMAGE_RECT_OFFSET), 8);
        }
    }
}

/// The host's `CLOCK_UPTIME_RAW` in nanoseconds (`CLOCK_MONOTONIC` off macOS).
pub(crate) fn uptime_ns() -> u64 {
    #[cfg(target_os = "macos")]
    let clock = libc::CLOCK_UPTIME_RAW;
    #[cfg(not(target_os = "macos"))]
    let clock = libc::CLOCK_MONOTONIC;
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    // SAFETY: `now` is a valid timespec for the call to fill.
    unsafe { libc::clock_gettime(clock, &mut now) };
    (now.tv_sec as u64)
        .saturating_mul(1_000_000_000)
        .saturating_add(now.tv_nsec as u64)
}

#[cfg(test)]
#[path = "fb_sink_header_tests.rs"]
mod tests;
