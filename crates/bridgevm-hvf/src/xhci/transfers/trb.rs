use crate::fwcfg::GuestMemoryMut;

use super::super::trace;

const TRB_SIZE: usize = 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1f_ffff;

#[derive(Clone, Copy)]
pub(super) struct TransferTrb {
    pub(super) gpa: u64,
    pub(super) parameter: u64,
    pub(super) status: u32,
    pub(super) control: u32,
}

pub(super) fn read_transfer_trb(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<TransferTrb> {
    let raw = mem.read_bytes(gpa, TRB_SIZE)?;
    Some(TransferTrb {
        gpa,
        parameter: read_u64(&raw, 0)?,
        status: read_u32(&raw, 8)?,
        control: read_u32(&raw, 12)?,
    })
}

pub(super) fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

pub(super) fn trb_transfer_length(status: u32) -> u32 {
    status & TRB_TRANSFER_LENGTH_MASK
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
