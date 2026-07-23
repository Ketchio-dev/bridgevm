//! Split out of nvme.rs to keep files under 850 lines.

use super::*;
use crate::fwcfg::GuestMemoryMut;
use std::sync::OnceLock;

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

// ---- small helpers --------------------------------------------------------

/// Mask `value` to a 1/2/4/8-byte access width.
pub(crate) fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}

pub(crate) fn is_modelled_doorbell(offset: u64) -> bool {
    (REG_DOORBELL_BASE..REG_DOORBELL_END).contains(&offset) && offset % 4 == 0
}

pub(crate) fn nvme_trace_enabled() -> bool {
    static ENABLED: OnceLock<bool> = OnceLock::new();
    *ENABLED.get_or_init(|| {
        matches!(
            std::env::var("BRIDGEVM_TRACE_NVME").ok().as_deref(),
            Some("1") | Some("true") | Some("TRUE") | Some("yes") | Some("YES")
        )
    })
}

pub(crate) fn identify_cns_name(cns: u32) -> &'static str {
    match cns {
        IDENTIFY_CNS_NAMESPACE => "namespace",
        IDENTIFY_CNS_CONTROLLER => "controller",
        IDENTIFY_CNS_ACTIVE_NAMESPACE_LIST => "active-ns-list",
        IDENTIFY_CNS_NAMESPACE_DESCRIPTOR_LIST => "ns-desc-list",
        IDENTIFY_CNS_COMMAND_SET_CONTROLLER => "command-set-controller",
        _ => "unknown",
    }
}

pub(crate) fn feature_capabilities(fid: u8) -> Option<u32> {
    match fid {
        FEATURE_TEMPERATURE_THRESHOLD
        | FEATURE_VOLATILE_WRITE_CACHE
        | FEATURE_NUMBER_OF_QUEUES
        | FEATURE_WRITE_ATOMICITY_NORMAL
        | FEATURE_ASYNC_EVENT_CONFIGURATION => Some(FEATURE_CAP_CHANGEABLE),
        FEATURE_ERROR_RECOVERY => Some(FEATURE_CAP_CHANGEABLE | FEATURE_CAP_NAMESPACE_SPECIFIC),
        FEATURE_ARBITRATION
        | FEATURE_POWER_MANAGEMENT
        | FEATURE_INTERRUPT_COALESCING
        | FEATURE_INTERRUPT_VECTOR_CONFIGURATION
        | FEATURE_AUTONOMOUS_POWER_STATE_TRANSITION => Some(0),
        _ => None,
    }
}

pub(crate) fn hex_preview(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        use std::fmt::Write;
        let _ = write!(&mut out, "{b:02x}");
    }
    out
}

pub(crate) fn rounded_disk_len(bytes: usize) -> usize {
    bytes.div_ceil(LBA_SIZE) * LBA_SIZE
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

/// Merge a partial write into a 64-bit register. `high` selects the upper
/// 32-bit half (for split 32-bit accesses to a 64-bit register).
pub(crate) fn merge_u64(current: u64, value: u64, size: u8, high: bool) -> u64 {
    if size >= 8 {
        return value;
    }
    if high {
        (current & 0x0000_0000_ffff_ffff) | (u64::from(value as u32) << 32)
    } else {
        (current & 0xffff_ffff_0000_0000) | u64::from(value as u32)
    }
}

/// Grow `slots` so index `idx` is addressable, filling new slots with `None`.
pub(crate) fn ensure_slot<T>(slots: &mut Vec<Option<T>>, idx: usize) {
    if idx >= slots.len() {
        slots.resize_with(idx + 1, || None);
    }
}

/// Copy `s` into `dst` as ASCII, space-padding the remainder (NVMe string
/// fields are space- not NUL-padded).
pub(crate) fn write_ascii(dst: &mut [u8], s: &str) {
    for b in dst.iter_mut() {
        *b = b' ';
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len());
    dst[..n].copy_from_slice(&bytes[..n]);
}

/// Copy `s` into `dst` as a C string, clearing the full destination first.
pub(crate) fn write_cstr(dst: &mut [u8], s: &str) {
    dst.fill(0);
    if dst.is_empty() {
        return;
    }
    let bytes = s.as_bytes();
    let n = bytes.len().min(dst.len() - 1);
    dst[..n].copy_from_slice(&bytes[..n]);
}

pub(crate) fn append_namespace_id_descriptor(dst: &mut [u8], off: &mut usize, nidt: u8, id: &[u8]) {
    let end = *off + 4 + id.len();
    assert!(end <= dst.len(), "namespace ID descriptor list overflow");
    dst[*off] = nidt;
    dst[*off + 1] = id.len() as u8;
    // bytes 2..4 are reserved and remain zero.
    dst[*off + 4..end].copy_from_slice(id);
    *off = end;
}
