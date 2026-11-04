use std::sync::OnceLock;

use crate::fwcfg::GuestMemoryMut;

use super::super::trace;

const TRB_SIZE: usize = 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1f_ffff;
const TRB_INTERRUPTER_TARGET_SHIFT: u32 = 22;
const TRB_TYPE_LINK: u32 = 6;
const LINK_TRB_POINTER_MASK: u64 = !0xf;

#[derive(Clone, Copy)]
pub(super) struct TransferTrb {
    pub(super) gpa: u64,
    pub(super) parameter: u64,
    pub(super) status: u32,
    pub(super) control: u32,
}

pub(super) fn read_transfer_trb(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<TransferTrb> {
    let mut raw = [0u8; TRB_SIZE];
    if !mem.read_into(gpa, &mut raw) {
        return None;
    }
    Some(TransferTrb {
        gpa,
        parameter: read_u64(&raw, 0)?,
        status: read_u32(&raw, 8)?,
        control: read_u32(&raw, 12)?,
    })
}

/// The TRB of a TD at `gpa`, or the one the Link TRB there points to: a TD
/// can cross the end of its ring segment between any two of its TRBs.
pub(super) fn read_td_trb(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<TransferTrb> {
    let trb = read_transfer_trb(mem, gpa)?;
    if trb_type(trb.control) != TRB_TYPE_LINK {
        return Some(trb);
    }
    trace_transfer_trb("link", trb);
    read_transfer_trb(mem, trb.parameter & LINK_TRB_POINTER_MASK)
}

/// A TD's TRB past its first: like `read_td_trb`, unless the trial baseline
/// `BRIDGEVM_XHCI_TD_LINKS=0` asks for the old walk, which followed a Link TRB
/// only in front of a control TD's first completion TRB.
pub(super) fn read_later_td_trb(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<TransferTrb> {
    static LINKS: OnceLock<bool> = OnceLock::new();
    let links = *LINKS
        .get_or_init(|| !matches!(std::env::var("BRIDGEVM_XHCI_TD_LINKS").as_deref(), Ok("0")));
    if links {
        read_td_trb(mem, gpa)
    } else {
        read_transfer_trb(mem, gpa)
    }
}

pub(super) fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

pub(super) fn trb_transfer_length(status: u32) -> u32 {
    status & TRB_TRANSFER_LENGTH_MASK
}

pub(super) fn trb_interrupter_target(status: u32) -> usize {
    (status >> TRB_INTERRUPTER_TARGET_SHIFT) as usize
}

pub(super) fn trace_transfer_trb(label: &str, trb: TransferTrb) {
    trace::ep0_trb(
        label,
        trb.gpa,
        trb.parameter,
        trb.status,
        trb.control,
        trb_type(trb.control),
    );
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let raw = bytes.get(offset..offset + 4)?;
    let array: [u8; 4] = raw.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

fn read_u64(bytes: &[u8], offset: usize) -> Option<u64> {
    let raw = bytes.get(offset..offset + 8)?;
    let array: [u8; 8] = raw.try_into().ok()?;
    Some(u64::from_le_bytes(array))
}
