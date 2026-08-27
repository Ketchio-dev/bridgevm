//! Caps control-TX and agent-TX doorbells at the active queue capacity.

pub(super) fn pending_entries(last_avail_idx: u16, avail_idx: u16, queue_size: u16) -> u16 {
    avail_idx.wrapping_sub(last_avail_idx).min(queue_size)
}
