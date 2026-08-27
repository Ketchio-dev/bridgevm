//! Caps control/cursor notifications at each queue's effective capacity.

pub(super) fn pending_entries(last_avail_idx: u16, avail_idx: u16, queue_size: u16) -> u16 {
    avail_idx.wrapping_sub(last_avail_idx).min(queue_size)
}
