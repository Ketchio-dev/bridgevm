//! PRP pointer/list decoding, LBA-to-byte transfer-range validation, and span coalescing.

use super::*;
use crate::fwcfg::GuestMemoryMut;

/// Decode a command's PRP data pointers into guest-physical spans covering
/// `len` bytes. PRP1 may start at an offset within the first memory page; PRP2
/// is either the second data page or, for larger transfers, a pointer into a PRP
/// list containing little-endian entries. The command's PRP2 list pointer may
/// itself include an offset into the first list page; chained list-page pointers
/// and data-page entries must be page-aligned.
pub(crate) fn prp_spans_into(
    cmd: &SubmissionEntry,
    len: usize,
    mem: &dyn GuestMemoryMut,
    out: &mut Vec<(u64, usize)>,
) -> bool {
    let start = out.len();
    if len == 0 {
        return true;
    }
    if cmd.prp1 == 0 {
        return false;
    }

    let mut remaining = len;
    let first_page_left = (PAGE_SIZE_U64 - (cmd.prp1 % PAGE_SIZE_U64)) as usize;
    let first_len = remaining.min(first_page_left);
    out.push((cmd.prp1, first_len));
    remaining -= first_len;

    if remaining == 0 {
        return true;
    }
    if cmd.prp2 == 0 {
        out.truncate(start);
        return false;
    }

    if remaining <= PAGE_SIZE {
        if cmd.prp2 % PAGE_SIZE_U64 != 0 {
            out.truncate(start);
            return false;
        }
        out.push((cmd.prp2, remaining));
        return true;
    }

    let mut list_gpa = cmd.prp2;
    let mut list_pages_seen = 0usize;

    while remaining > 0 {
        let list_offset = (list_gpa % PAGE_SIZE_U64) as usize;
        if list_offset % 8 != 0 {
            out.truncate(start);
            return false;
        }
        list_pages_seen += 1;
        if list_pages_seen > 16 {
            out.truncate(start);
            return false;
        }

        let list_len = PAGE_SIZE - list_offset;
        let mut list_buf = [0u8; PAGE_SIZE];
        if !mem.read_into(list_gpa, &mut list_buf[..list_len]) {
            out.truncate(start);
            return false;
        }
        let raw = &list_buf[..list_len];
        let mut followed_chain = false;
        let entries_in_page = raw.len() / 8;
        for (idx, chunk) in raw.chunks_exact(8).enumerate() {
            let entry = u64::from_le_bytes(chunk.try_into().unwrap());
            if entry == 0 {
                out.truncate(start);
                return false;
            }

            if idx == entries_in_page - 1 && remaining > PAGE_SIZE {
                if entry % PAGE_SIZE_U64 != 0 {
                    out.truncate(start);
                    return false;
                }
                list_gpa = entry;
                followed_chain = true;
                break;
            }

            if entry % PAGE_SIZE_U64 != 0 {
                out.truncate(start);
                return false;
            }
            let span_len = remaining.min(PAGE_SIZE);
            out.push((entry, span_len));
            remaining -= span_len;
            if remaining == 0 {
                return true;
            }
        }

        if !followed_chain {
            out.truncate(start);
            return false;
        }
    }

    true
}

/// Decode (SLBA, NLB) into a byte range, validating it fits a `byte_len`-sized
/// namespace. Returns `(start_byte, len_bytes)`.
pub(crate) fn transfer_range(cmd: &SubmissionEntry, byte_len: u64) -> Option<(u64, usize)> {
    let slba = u64::from(cmd.cdw10) | (u64::from(cmd.cdw11) << 32);
    let nlb = u64::from(cmd.cdw12 & 0xffff) + 1; // 0-based count
    let len = nlb.checked_mul(LBA_SIZE as u64)?;
    let start = slba.checked_mul(LBA_SIZE as u64)?;
    if start.checked_add(len)? > byte_len {
        return None; // out of range
    }
    if len > usize::MAX as u64 {
        return None;
    }
    Some((start, len as usize))
}

/// Merge physically-contiguous PRP spans into larger segments. The spans arrive
/// in transfer order and the disk range is contiguous, so any run whose guest
/// addresses abut collapses into one segment — turning a 128 KiB scatter of
/// thirty-two 4 KiB pages into a single segment when the guest allocated the
/// buffer contiguously. `checked_add` keeps a guest-controlled address near the
/// top of the space from overflowing (and, under overflow-checks, panicking).
#[cfg(test)]
pub(crate) fn coalesce_spans(spans: &[(u64, usize)]) -> Vec<(u64, usize)> {
    let mut out = Vec::with_capacity(spans.len());
    coalesce_spans_into(spans, &mut out);
    out
}

pub(crate) fn coalesce_spans_into(spans: &[(u64, usize)], out: &mut Vec<(u64, usize)>) {
    out.reserve(spans.len());
    for &(gpa, len) in spans {
        if len == 0 {
            continue;
        }
        if let Some(last) = out.last_mut() {
            if last.0.checked_add(last.1 as u64) == Some(gpa) {
                // Segment length is bounded by the transfer length, which
                // transfer_range already validated fits the namespace.
                last.1 += len;
                continue;
            }
        }
        out.push((gpa, len));
    }
}
