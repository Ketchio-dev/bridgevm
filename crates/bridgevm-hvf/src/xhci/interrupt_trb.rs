use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

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
const SLOT_ID: u32 = 1;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
/// xHCI completion code 13 for a TD shorter than the requested transfer.
const COMPLETION_CODE_SHORT_PACKET: u32 = 13;

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

/// The Event Data TRB a Normal TRB chains to.
pub(super) struct ChainedEventData {
    pub(super) trb: InterruptTransferTrb,
    /// The TD crossed a Link TRB with Toggle Cycle set: the consumer cycle
    /// state flips for every TRB after it.
    pub(super) toggles_cycle: bool,
}

/// When the Normal TRB is the last slot before its segment's Link TRB, the TD
/// crosses the Link TRB and the Event Data TRB is the first slot it points
/// to, in the next cycle pass if the Link TRB toggles the cycle bit.
pub(super) fn read_chained_event_data(
    mem: &dyn GuestMemoryMut,
    normal: &InterruptTransferTrb,
    dcs: bool,
) -> Option<ChainedEventData> {
    if normal.control & TRB_CHAIN == 0 {
        return None;
    }
    let mut trb = read_transfer_trb(mem, normal.gpa.checked_add(TRB_SIZE_BYTES)?)?;
    let mut cycle = dcs;
    let mut toggles_cycle = false;
    if trb_type(trb.control) == TRB_TYPE_LINK && has_cycle(&trb, cycle) {
        toggles_cycle = trb.control & TRB_LINK_TOGGLE_CYCLE != 0;
        cycle ^= toggles_cycle;
        trb = read_transfer_trb(mem, trb.parameter & LINK_TRB_POINTER_MASK)?;
    }
    if !has_cycle(&trb, cycle)
        || trb_type(trb.control) != TRB_TYPE_EVENT_DATA
        || trb.control & TRB_IOC == 0
    {
        return None;
    }
    Some(ChainedEventData { trb, toggles_cycle })
}

/// What [`XhciController::fill_interrupt_in_td`] did.
pub(super) struct InterruptInTdFill {
    pub(super) written_length: u32,
    pub(super) completion_code: u32,
    pub(super) posted: bool,
}

impl XhciController {
    /// Writes `report`, cut to the Normal TRB's transfer length, into the
    /// TRB's buffer and posts the TD's Transfer Event for slot 1's
    /// `endpoint_id`. A TD that ends in an Event Data TRB completes with one
    /// Event Data event (ED=1) carrying the bytes written, Short Packet when
    /// the buffer was longer than the report. Otherwise the event points at
    /// the Normal TRB and carries the residual length; its code is Short
    /// Packet for a short TD only if `short_packet_without_event_data`, else
    /// Success. `None` when the buffer write failed and nothing was posted.
    pub(super) fn fill_interrupt_in_td(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        endpoint_id: u32,
        normal: &InterruptTransferTrb,
        event_data: Option<&ChainedEventData>,
        report: &[u8],
        short_packet_without_event_data: bool,
    ) -> Option<InterruptInTdFill> {
        let transfer_length = trb_transfer_length(normal.status);
        let written_length = transfer_length.min(report.len() as u32);
        if written_length > 0
            && !mem.write_bytes(normal.parameter, &report[..written_length as usize])
        {
            return None;
        }
        let completion_code = if written_length < transfer_length {
            COMPLETION_CODE_SHORT_PACKET
        } else {
            COMPLETION_CODE_SUCCESS
        };
        let event_control = transfer_event_control(SLOT_ID, endpoint_id);
        let posted = match event_data {
            Some(event_data) => self.post_event_to_interrupter(
                mem,
                trb_interrupter_target(event_data.trb.status),
                event_data.trb.parameter,
                (completion_code << COMPLETION_CODE_SHIFT) | written_length,
                event_control | TRANSFER_EVENT_ED,
            ),
            None => {
                let code = if short_packet_without_event_data {
                    completion_code
                } else {
                    COMPLETION_CODE_SUCCESS
                };
                self.post_event_to_interrupter(
                    mem,
                    trb_interrupter_target(normal.status),
                    normal.gpa,
                    (code << COMPLETION_CODE_SHIFT) | (transfer_length - written_length),
                    event_control,
                )
            }
        };
        Some(InterruptInTdFill {
            written_length,
            completion_code,
            posted,
        })
    }
}

fn has_cycle(trb: &InterruptTransferTrb, cycle: bool) -> bool {
    (trb.control & TRB_CYCLE != 0) == cycle
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
