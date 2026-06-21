use crate::fwcfg::GuestMemoryMut;

use super::XhciController;

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
const EP0_CONTEXT_OFFSET: u64 = 0x40;
const EP_TR_DEQUEUE_OFFSET: u64 = 0x8;
const EP_TR_DEQUEUE_MASK: u64 = !0xf;
const USB_REQUEST_GET_DESCRIPTOR: u8 = 0x06;
const USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE: u8 = 0x80;
const USB_DESCRIPTOR_TYPE_DEVICE: u8 = 1;
const DEVICE_DESCRIPTOR_LENGTH: u16 = 18;
const TRB_TRANSFER_LENGTH_MASK: u32 = 0x1f_ffff;

const DEVICE_DESCRIPTOR: [u8; 18] = [
    18, 1, 0x00, 0x02, 0, 0, 0, 64, 0x09, 0x12, 0x01, 0x00, 0x00, 0x01, 0, 0, 0, 1,
];

#[derive(Clone, Copy)]
struct TransferTrb {
    gpa: u64,
    parameter: u64,
    status: u32,
    control: u32,
}

#[derive(Clone, Copy)]
struct SetupPacket {
    bm_request_type: u8,
    request: u8,
    value: u16,
    index: u16,
    length: u16,
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
    pub(super) fn capture_address_device_input_context(
        &mut self,
        mem: &dyn GuestMemoryMut,
        input_context: u64,
        slot_id: u32,
    ) {
        if slot_id != SLOT_ID {
            return;
        }
        let ep0_dequeue = input_context + EP0_CONTEXT_OFFSET + EP_TR_DEQUEUE_OFFSET;
        let Some(raw_dequeue) = read_mem_u64(mem, ep0_dequeue) else {
            return;
        };
        self.slot1_ep0_dequeue = raw_dequeue & EP_TR_DEQUEUE_MASK;
    }

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
        if transfer_ring == 0 {
            return false;
        }
        let Some(setup) = read_transfer_trb(mem, transfer_ring) else {
            return false;
        };
        let Some(data) = read_transfer_trb(mem, transfer_ring + TRB_SIZE_BYTES) else {
            return false;
        };
        let Some(completion) = find_control_completion(mem, transfer_ring + 2 * TRB_SIZE_BYTES)
        else {
            return false;
        };
        let setup_packet = parse_setup_packet(setup.parameter);
        if !is_device_descriptor_request(setup_packet)
            || trb_type(setup.control) != TRB_TYPE_SETUP_STAGE
            || trb_type(data.control) != TRB_TYPE_DATA_STAGE
            || data.control & TRB_DATA_STAGE_DIRECTION_IN == 0
            || trb_transfer_length(data.status) != u32::from(DEVICE_DESCRIPTOR_LENGTH)
        {
            return false;
        }
        if setup_packet.length != DEVICE_DESCRIPTOR_LENGTH {
            return false;
        }
        if !mem.write_bytes(data.parameter, &DEVICE_DESCRIPTOR) {
            return false;
        }
        let (event_parameter, event_length, event_flags) =
            transfer_event_completion(&completion, trb_transfer_length(data.status));
        let posted = self.post_event(
            mem,
            event_parameter,
            (COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT) | event_length,
            transfer_event_control(SLOT_ID, ENDPOINT_ID_EP0) | event_flags,
        );
        if posted {
            self.slot1_ep0_dequeue = completion.status_stage.gpa + TRB_SIZE_BYTES;
        }
        posted
    }
}

fn find_control_completion(mem: &dyn GuestMemoryMut, first_gpa: u64) -> Option<ControlCompletion> {
    let first = read_transfer_trb(mem, first_gpa)?;
    match trb_type(first.control) {
        TRB_TYPE_STATUS_STAGE => Some(ControlCompletion {
            status_stage: first,
            event_data: None,
        }),
        TRB_TYPE_EVENT_DATA => {
            let second = read_transfer_trb(mem, first_gpa + TRB_SIZE_BYTES)?;
            match trb_type(second.control) {
                TRB_TYPE_STATUS_STAGE => Some(ControlCompletion {
                    status_stage: second,
                    event_data: (first.control & TRB_IOC != 0).then_some(first),
                }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn transfer_event_completion(completion: &ControlCompletion, transferred: u32) -> (u64, u32, u32) {
    if let Some(event_data) = completion.event_data {
        (event_data.parameter, transferred, TRANSFER_EVENT_ED)
    } else {
        (completion.status_stage.gpa, 0, 0)
    }
}

fn is_device_descriptor_request(packet: SetupPacket) -> bool {
    let [descriptor_index, descriptor_type] = packet.value.to_le_bytes();
    packet.bm_request_type == USB_REQUEST_TYPE_DEVICE_TO_HOST_STANDARD_DEVICE
        && packet.request == USB_REQUEST_GET_DESCRIPTOR
        && descriptor_type == USB_DESCRIPTOR_TYPE_DEVICE
        && descriptor_index == 0
        && packet.index == 0
}

fn transfer_event_control(slot_id: u32, endpoint_id: u32) -> u32 {
    (slot_id << EVENT_SLOT_ID_SHIFT)
        | (endpoint_id << EVENT_ENDPOINT_ID_SHIFT)
        | (TRB_TYPE_TRANSFER_EVENT << TRB_TYPE_SHIFT)
}

fn parse_setup_packet(parameter: u64) -> SetupPacket {
    let bytes = parameter.to_le_bytes();
    SetupPacket {
        bm_request_type: bytes[0],
        request: bytes[1],
        value: u16::from_le_bytes([bytes[2], bytes[3]]),
        index: u16::from_le_bytes([bytes[4], bytes[5]]),
        length: u16::from_le_bytes([bytes[6], bytes[7]]),
    }
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

fn read_mem_u64(mem: &dyn GuestMemoryMut, gpa: u64) -> Option<u64> {
    let raw = mem.read_bytes(gpa, 8)?;
    read_u64(&raw, 0)
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
