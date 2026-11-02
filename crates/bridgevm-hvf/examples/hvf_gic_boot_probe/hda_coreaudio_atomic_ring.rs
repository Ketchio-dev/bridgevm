//! Fixed-capacity PCM transport: one serialized producer and a non-waiting consumer.
//! Atomic frame slots avoid both unsafe shared storage and producer/callback locks.

use super::hda_coreaudio_publication::PcmQueue;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering};

const FRAME_BYTES: usize = 4;

pub(super) struct AtomicPcmRing {
    slots: Box<[AtomicU32]>,
    logical_frames: usize,
    mask: usize,
    head: AtomicUsize,
    tail: AtomicUsize,
    consuming: AtomicBool,
}

impl AtomicPcmRing {
    pub(super) fn new(capacity_bytes: usize) -> Self {
        let logical_frames = capacity_bytes / FRAME_BYTES;
        // The logical capacity is unchanged by physical allocation rounding.
        // A power-of-two mask divides the usize counter period, including wrap.
        let physical_frames = logical_frames.max(1).next_power_of_two();
        Self {
            slots: (0..physical_frames).map(|_| AtomicU32::new(0)).collect(),
            logical_frames,
            mask: physical_frames - 1,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
            consuming: AtomicBool::new(false),
        }
    }

    /// Only the owning sink's serialized producer may publish to this ring.
    pub(super) fn producer(&self) -> Producer<'_> {
        Producer { ring: self }
    }

    /// One callback owns the read cursor at a time, without waiting on either
    /// another callback or the producer. Its published-head snapshot never grows.
    pub(super) fn try_consumer(&self) -> Option<Consumer<'_>> {
        self.consuming
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()?;
        let cursor = self.tail.load(Ordering::Relaxed);
        let published = self.head.load(Ordering::Acquire);
        Some(Consumer {
            ring: self,
            cursor,
            published,
        })
    }

    /// A bounded observation for contracts; actual playback uses a captured head.
    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        let tail = self.tail.load(Ordering::Acquire);
        let head = self.head.load(Ordering::Acquire);
        head.wrapping_sub(tail).min(self.logical_frames) * FRAME_BYTES
    }

    #[cfg(test)]
    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

pub(super) struct Producer<'a> {
    ring: &'a AtomicPcmRing,
}

impl Producer<'_> {
    /// Reserve the whole batch and copy it into free slots without exposing it.
    /// The returned borrow prevents this producer from publishing another batch.
    fn prepare_parts(
        &mut self,
        first: &[u8],
        second: &[u8],
        capacity: usize,
    ) -> Option<PendingPublication<'_>> {
        let bytes = first.len().checked_add(second.len())?;
        if bytes % FRAME_BYTES != 0 {
            return None;
        }
        let frames = bytes / FRAME_BYTES;
        let head = self.ring.head.load(Ordering::Relaxed);
        let used = head.wrapping_sub(self.ring.tail.load(Ordering::Acquire));
        let limit = self.ring.logical_frames.min(capacity / FRAME_BYTES);
        if frames > limit.saturating_sub(used) {
            return None;
        }
        let byte = |offset: usize| {
            if offset < first.len() {
                first[offset]
            } else {
                second[offset - first.len()]
            }
        };
        for frame in 0..frames {
            let offset = frame * FRAME_BYTES;
            let word = u32::from_le_bytes(std::array::from_fn(|i| byte(offset + i)));
            self.ring.slots[head.wrapping_add(frame) & self.ring.mask]
                .store(word, Ordering::Relaxed);
        }
        Some(PendingPublication {
            ring: self.ring,
            end: head.wrapping_add(frames),
        })
    }
}

impl PcmQueue for Producer<'_> {
    fn len(&self) -> usize {
        let head = self.ring.head.load(Ordering::Relaxed);
        head.wrapping_sub(self.ring.tail.load(Ordering::Acquire)) * FRAME_BYTES
    }

    fn publish_parts(&mut self, first: &[u8], second: &[u8], capacity: usize) -> bool {
        let Some(publication) = self.prepare_parts(first, second, capacity) else {
            return false;
        };
        publication.commit();
        true
    }
}

struct PendingPublication<'a> {
    ring: &'a AtomicPcmRing,
    end: usize,
}

impl PendingPublication<'_> {
    fn commit(self) {
        // Publish every frame in the batch together, after all slot stores.
        self.ring.head.store(self.end, Ordering::Release);
    }
}

pub(super) struct Consumer<'a> {
    ring: &'a AtomicPcmRing,
    cursor: usize,
    published: usize,
}

impl Consumer<'_> {
    pub(super) fn len(&self) -> usize {
        self.published.wrapping_sub(self.cursor) * FRAME_BYTES
    }

    pub(super) fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub(super) fn drain_into(&mut self, destination: &mut [u8]) -> usize {
        let frames = (self.len() / FRAME_BYTES).min(destination.len() / FRAME_BYTES);
        for (i, chunk) in destination[..frames * FRAME_BYTES]
            .chunks_exact_mut(FRAME_BYTES)
            .enumerate()
        {
            let word = self.ring.slots[self.cursor.wrapping_add(i) & self.ring.mask]
                .load(Ordering::Relaxed);
            chunk.copy_from_slice(&word.to_le_bytes());
        }
        self.cursor = self.cursor.wrapping_add(frames);
        // Reuse becomes legal only after the destination has received all bytes.
        self.ring.tail.store(self.cursor, Ordering::Release);
        frames * FRAME_BYTES
    }
}

impl Drop for Consumer<'_> {
    fn drop(&mut self) {
        self.ring.consuming.store(false, Ordering::Release);
    }
}

#[cfg(test)]
#[path = "hda_coreaudio_atomic_ring_tests.rs"]
mod tests;
