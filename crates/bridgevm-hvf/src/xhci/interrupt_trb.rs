use crate::fwcfg::GuestMemoryMut;

pub(super) const TRB_SIZE_BYTES: u64 = 16;
pub(super) const TRB_CYCLE: u32 = 1;
pub(super) const TRB_LINK_TOGGLE_CYCLE: u32 = 1 << 1;
pub(super) const TRB_TYPE_LINK: u32 = 6;
pub(super) const TRB_TYPE_NORMAL: u32 = 1;
pub(super) const LINK_TRB_POINTER_MASK: u64 = !0xf;
pub(super) const COMPLETION_CODE_SUCCESS: u32 = 1;
pub(super) const COMPLETION_CODE_SHIFT: u32 = 24;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const TRB_TYPE_EVENT_DATA: u32 = 7;

const TRB_SIZE: usize = 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1f_ffff;
const TRB_INTERRUPTER_TARGET_SHIFT: u32 = 22;
const EVENT_ENDPOINT_ID_SHIFT: u32 = 16;
const EVENT_SLOT_ID_SHIFT: u32 = 24;

pub(super) struct InterruptTransferTrb {
    pub(super) gpa: u64,
    pub(super) parameter: u64,
    pub(super) status: u32,
    pub(super) control: u32,
}

pub(super) fn read_transfer_trb(
    mem: &dyn GuestMemoryMut,
    gpa: u64,
) -> Option<InterruptTransferTrb> {
    let mut raw = [0u8; TRB_SIZE];
    if !mem.read_into(gpa, &mut raw) {
        return None;
    }
    Some(InterruptTransferTrb {
        gpa,
        parameter: read_u64(&raw, 0)?,
        status: read_u32(&raw, 8)?,
        control: read_u32(&raw, 12)?,
    })
}

pub(super) const fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

pub(super) const fn trb_transfer_length(status: u32) -> u32 {
    status & TRB_TRANSFER_LENGTH_MASK
}

pub(super) const fn trb_interrupter_target(status: u32) -> usize {
    (status >> TRB_INTERRUPTER_TARGET_SHIFT) as usize
}

pub(super) const fn transfer_event_control(slot_id: u32, endpoint_id: u32) -> u32 {
    (slot_id << EVENT_SLOT_ID_SHIFT)
        | (endpoint_id << EVENT_ENDPOINT_ID_SHIFT)
        | (TRB_TYPE_TRANSFER_EVENT << TRB_TYPE_SHIFT)
}

pub(super) fn read_chained_event_data(
    mem: &dyn GuestMemoryMut,
    normal: &InterruptTransferTrb,
    dcs: bool,
) -> Option<InterruptTransferTrb> {
    if normal.control & TRB_CHAIN == 0 {
        return None;
    }
    let event_data_gpa = normal.gpa.checked_add(TRB_SIZE_BYTES)?;
    let event_data = read_transfer_trb(mem, event_data_gpa)?;
    let expected_cycle = if dcs { TRB_CYCLE } else { 0 };
    if event_data.control & TRB_CYCLE != expected_cycle
        || trb_type(event_data.control) != TRB_TYPE_EVENT_DATA
        || event_data.control & TRB_IOC == 0
    {
        return None;
    }
    Some(event_data)
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
