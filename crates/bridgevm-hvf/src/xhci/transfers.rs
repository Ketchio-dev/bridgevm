use crate::fwcfg::GuestMemoryMut;

use super::{
    trace,
    usb::{
        is_device_descriptor_request, parse_setup_packet, DEVICE_DESCRIPTOR,
        DEVICE_DESCRIPTOR_LENGTH,
    },
    XhciController,
};

const DOORBELL_BASE: u64 = 0x2000;
const DOORBELL_STRIDE: u64 = 4;
const TRB_SIZE: usize = 16;
const TRB_SIZE_BYTES: u64 = 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_MASK: u32 = 0x3f;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
const TRB_IOC: u32 = 1 << 5;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const COMPLETION_CODE_SHIFT: u32 = 24;
const SLOT_ID: u32 = 1;
const ENDPOINT_ID_EP0: u32 = 1;
const EVENT_ENDPOINT_ID_SHIFT: u32 = 16;
const EVENT_SLOT_ID_SHIFT: u32 = 24;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1f_ffff;

#[derive(Clone, Copy)]
struct TransferTrb {
    gpa: u64,
    parameter: u64,
    status: u32,
    control: u32,
}

#[derive(Clone, Copy)]
struct ControlCompletion {
    status_stage: TransferTrb,
    event_data: Option<TransferTrb>,
}

pub(super) const fn is_slot_doorbell(offset: u64, size: u8) -> bool {
    offset == DOORBELL_BASE + DOORBELL_STRIDE && size == 4
}

impl XhciController {
    pub(super) fn process_slot_doorbell(
        &mut self,
        offset: u64,
        value: u64,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        let Some(slot_offset) = offset.checked_sub(DOORBELL_BASE) else {
            return false;
        };
        let Ok(slot_id) = u32::try_from(slot_offset / DOORBELL_STRIDE) else {
            return false;
        };
        let Ok(target) = u32::try_from(value & 0xff) else {
            return false;
        };
        if slot_id != SLOT_ID || target != ENDPOINT_ID_EP0 {
            return false;
        }
        self.process_ep0_control_transfer(mem)
    }

    fn process_ep0_control_transfer(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        let transfer_ring = self.slot1_ep0_dequeue;
        trace::ep0_handler_entered(transfer_ring);
        if transfer_ring == 0 {
            trace::ep0_reject("no_ep0_dequeue");
            return false;
        }
        let Some(setup) = read_transfer_trb(mem, transfer_ring) else {
            trace::ep0_reject_with_gpa("setup_trb_read_failed", transfer_ring);
            return false;
        };
        trace_trb("setup", setup);
        let Some(data) = read_transfer_trb(mem, transfer_ring + TRB_SIZE_BYTES) else {
            trace::ep0_reject_with_gpa("data_trb_read_failed", transfer_ring + TRB_SIZE_BYTES);
            return false;
        };
        trace_trb("data", data);
        let Some(completion) = find_control_completion(mem, transfer_ring + 2 * TRB_SIZE_BYTES)
        else {
            trace::ep0_reject_with_gpa(
                "completion_trbs_invalid",
                transfer_ring + 2 * TRB_SIZE_BYTES,
            );
            return false;
        };
        let setup_packet = parse_setup_packet(setup.parameter);
        trace::ep0_setup_packet(
            setup_packet.bm_request_type,
            setup_packet.request,
            setup_packet.value,
            setup_packet.index,
            setup_packet.length,
        );
        let setup_type = trb_type(setup.control);
        if !is_device_descriptor_request(setup_packet) {
            trace::ep0_reject("unsupported_setup_packet");
            return false;
        }
        if setup_type != TRB_TYPE_SETUP_STAGE {
            trace::ep0_reject_with_value("unexpected_setup_trb_type", setup_type);
            return false;
        }
        let data_type = trb_type(data.control);
        if data_type != TRB_TYPE_DATA_STAGE {
            trace::ep0_reject_with_value("unexpected_data_trb_type", data_type);
            return false;
        }
        if data.control & TRB_DATA_STAGE_DIRECTION_IN == 0 {
            trace::ep0_reject("data_stage_not_in");
            return false;
        }
        let data_length = trb_transfer_length(data.status);
        if data_length == 0 || data_length > u32::from(DEVICE_DESCRIPTOR_LENGTH) {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        }
        if u32::from(setup_packet.length) != data_length {
            trace::ep0_reject_with_value("unexpected_setup_length", u32::from(setup_packet.length));
            return false;
        }
        let Ok(descriptor_length) = usize::try_from(data_length) else {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        };
        let descriptor = &DEVICE_DESCRIPTOR[..descriptor_length];
        if !mem.write_bytes(data.parameter, descriptor) {
            trace::ep0_reject_with_gpa("descriptor_write_failed", data.parameter);
            return false;
        }
        trace::ep0_descriptor_write_success(data.parameter, descriptor.len());
        let start_event_status = COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT;
        let start_event_control = transfer_event_control(SLOT_ID, ENDPOINT_ID_EP0);
        trace::ep0_post_event_request(setup.gpa, start_event_status, start_event_control);
        let start_posted = self.post_event(mem, setup.gpa, start_event_status, start_event_control);
        trace::ep0_post_event_result(start_posted);
        if !start_posted {
            return false;
        }
        let (event_parameter, event_residual_length, event_flags) =
            transfer_event_completion(&completion);
        let event_status =
            (COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT) | event_residual_length;
        let event_control = transfer_event_control(SLOT_ID, ENDPOINT_ID_EP0) | event_flags;
        trace::ep0_post_event_request(event_parameter, event_status, event_control);
        let posted = self.post_event(mem, event_parameter, event_status, event_control);
        trace::ep0_post_event_result(posted);
        if posted {
            self.slot1_ep0_dequeue = completion.status_stage.gpa + TRB_SIZE_BYTES;
        }
        posted
    }
}

fn find_control_completion(mem: &dyn GuestMemoryMut, first_gpa: u64) -> Option<ControlCompletion> {
    let first = read_transfer_trb(mem, first_gpa)?;
    match trb_type(first.control) {
        TRB_TYPE_STATUS_STAGE => {
            trace_trb("status", first);
            Some(ControlCompletion {
                status_stage: first,
                event_data: None,
            })
        }
        TRB_TYPE_EVENT_DATA => {
            trace_trb("event_data", first);
            let second = read_transfer_trb(mem, first_gpa + TRB_SIZE_BYTES)?;
            trace_trb("status", second);
            match trb_type(second.control) {
                TRB_TYPE_STATUS_STAGE => Some(ControlCompletion {
                    status_stage: second,
                    event_data: (first.control & TRB_IOC != 0).then_some(first),
                }),
                _ => {
                    trace::ep0_reject_with_value(
                        "completion_second_not_status",
                        trb_type(second.control),
                    );
                    None
                }
            }
        }
        _ => {
            trace::ep0_reject_with_value(
                "completion_first_unexpected_type",
                trb_type(first.control),
            );
            None
        }
    }
}

fn transfer_event_completion(completion: &ControlCompletion) -> (u64, u32, u32) {
    if let Some(event_data) = completion.event_data {
        (event_data.parameter, 0, TRANSFER_EVENT_ED)
    } else {
        (completion.status_stage.gpa, 0, 0)
    }
}

fn transfer_event_control(slot_id: u32, endpoint_id: u32) -> u32 {
    (slot_id << EVENT_SLOT_ID_SHIFT)
        | (endpoint_id << EVENT_ENDPOINT_ID_SHIFT)
        | (TRB_TYPE_TRANSFER_EVENT << TRB_TYPE_SHIFT)
}

fn read_transfer_trb(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<TransferTrb> {
    let raw = mem.read_bytes(gpa, TRB_SIZE)?;
    Some(TransferTrb {
        gpa,
        parameter: read_u64(&raw, 0)?,
        status: read_u32(&raw, 8)?,
        control: read_u32(&raw, 12)?,
    })
}

fn trb_type(control: u32) -> u32 {
    (control >> TRB_TYPE_SHIFT) & TRB_TYPE_MASK
}

fn trb_transfer_length(status: u32) -> u32 {
    status & TRB_TRANSFER_LENGTH_MASK
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

fn trace_trb(label: &str, trb: TransferTrb) {
    trace::ep0_trb(
        label,
        trb.gpa,
        trb.parameter,
        trb.status,
        trb.control,
        trb_type(trb.control),
    );
}
