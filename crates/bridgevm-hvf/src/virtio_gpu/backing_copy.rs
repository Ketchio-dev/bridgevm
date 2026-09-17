//! Allocation-free reads from fragmented virtio-gpu resource backing.

use super::BackingEntry;
use crate::fwcfg::GuestMemoryMut;

pub(crate) fn read_from_backing_into(
    mem: &dyn GuestMemoryMut,
    backing: &[BackingEntry],
    offset: u64,
    dst: &mut [u8],
) -> bool {
    let Ok(len) = u64::try_from(dst.len()) else {
        return false;
    };
    let Some(range_end) = offset.checked_add(len) else {
        return false;
    };
    let mut base = 0u64;
    let mut start_entry = None;
    let mut covered = None;
    for (index, entry) in backing.iter().enumerate() {
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
    }
    let Some((start, mut base, end)) = covered else {
        return false;
    };

    let mut copied = 0usize;
    for entry in &backing[start..end] {
        let entry_end = base + u64::from(entry.len);
        let logical_start = offset.max(base);
        let logical_end = range_end.min(entry_end);
        if logical_start < logical_end {
            let dst_start = usize::try_from(logical_start - offset).unwrap();
            let chunk_len = usize::try_from(logical_end - logical_start).unwrap();
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
#[path = "backing_copy_tests.rs"]
mod tests;
