//! Allocation-free helpers for bounded reads from guest RAM into reusable buffers.

use crate::fwcfg::GuestMemoryMut;

pub(crate) fn append_guest_bytes_bounded(
    mem: &dyn GuestMemoryMut,
    gpa: u64,
    len: usize,
    max_len: usize,
    out: &mut Vec<u8>,
) -> bool {
    let start = out.len();
    let Some(end) = start.checked_add(len) else {
        return false;
    };
    if end > max_len {
        return false;
    }
    out.resize(end, 0);
    if mem.read_into(gpa, &mut out[start..end]) {
        return true;
    }
    out.truncate(start);
    false
}
