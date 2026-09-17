//! Sequential range reads over fragmented virtio-gpu backing.

use super::BackingEntry;
use crate::fwcfg::GuestMemoryMut;

#[derive(Default)]
pub(crate) struct BackingReadCursor {
    index: usize,
    base: u64,
}

pub(crate) fn read_from_backing_into_from(
    mem: &dyn GuestMemoryMut,
    backing: &[BackingEntry],
    offset: u64,
    dst: &mut [u8],
    cursor: &mut BackingReadCursor,
) -> bool {
    let Ok(len) = u64::try_from(dst.len()) else {
        return false;
    };
    let Some(range_end) = offset.checked_add(len) else {
        return false;
    };
    let reusable = !dst.is_empty() && cursor.index <= backing.len() && offset >= cursor.base;
    let (mut index, mut base) = if reusable {
        (cursor.index, cursor.base)
    } else {
        (0, 0)
    };
    let mut start_entry = None;
    let mut covered = None;
    while let Some(entry) = backing.get(index) {
        let Some(entry_end) = base.checked_add(u64::from(entry.len)) else {
            return false;
        };
        if len == 0 && offset >= base && offset <= entry_end {
            let Some(gpa) = entry.addr.checked_add(offset - base) else {
                return false;
            };
            return mem.read_into(gpa, dst);
        }
        if start_entry.is_none() && offset >= base && offset < entry_end {
            start_entry = Some((index, base));
        }
        if let Some((start, start_base)) = start_entry {
            if range_end <= entry_end {
                covered = Some((start, start_base, index + 1));
                break;
            }
        }
        base = entry_end;
        index += 1;
    }
    let Some((start, mut base, end)) = covered else {
        return false;
    };
    cursor.index = start;
    cursor.base = base;

    let mut copied = 0usize;
    for entry in &backing[start..end] {
        let Some(entry_end) = base.checked_add(u64::from(entry.len)) else {
            return false;
        };
        let logical_start = offset.max(base);
        let logical_end = range_end.min(entry_end);
        if logical_start < logical_end {
            let Ok(dst_start) = usize::try_from(logical_start - offset) else {
                return false;
            };
            let Ok(chunk_len) = usize::try_from(logical_end - logical_start) else {
                return false;
            };
            let Some(gpa) = entry.addr.checked_add(logical_start - base) else {
                return false;
            };
            if !mem.read_into(gpa, &mut dst[dst_start..dst_start + chunk_len]) {
                return false;
            }
            copied += chunk_len;
        }
        base = entry_end;
    }
    copied == dst.len()
}

#[cfg(test)]
#[path = "backing_copy_cursor_tests.rs"]
mod tests;
