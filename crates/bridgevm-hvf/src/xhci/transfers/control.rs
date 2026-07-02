use crate::fwcfg::GuestMemoryMut;

use super::super::{
    trace,
    usb::{
        data_in_for_setup_packet, is_hid_set_protocol_request, is_hid_set_report_request,
        parse_setup_packet, set_configuration_value, ControlInData,
    },
    XhciController,
};
use super::completion::{find_control_completion, ControlEventRequest};
use super::trb::{
    read_transfer_trb, trace_transfer_trb, trb_transfer_length, trb_type, TransferTrb,
};

const TRB_SIZE_BYTES: u64 = 16;
const TRB_TYPE_SETUP_STAGE: u32 = 2;
const TRB_TYPE_DATA_STAGE: u32 = 3;
const TRB_DATA_STAGE_DIRECTION_IN: u32 = 1 << 16;
const DATA_STAGE_OFFSET: u64 = TRB_SIZE_BYTES;
const STATUS_STAGE_OFFSET: u64 = TRB_SIZE_BYTES * 2;

impl XhciController {
    pub(super) fn process_ep0_control_transfer(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
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
        trace_transfer_trb("setup", setup);
        let setup_packet = parse_setup_packet(setup.parameter);
        trace::ep0_setup_packet(
            setup_packet.bm_request_type,
            setup_packet.request,
            setup_packet.value,
            setup_packet.index,
            setup_packet.length,
        );
        let setup_type = trb_type(setup.control);
        if setup_type != TRB_TYPE_SETUP_STAGE {
            trace::ep0_reject_with_value("unexpected_setup_trb_type", setup_type);
            return false;
        }
        let set_configuration = set_configuration_value(setup_packet);
        if set_configuration.is_some() || is_hid_set_protocol_request(setup_packet) {
            let Some(completion_gpa) = transfer_ring.checked_add(DATA_STAGE_OFFSET) else {
                trace::ep0_reject_with_gpa("completion_trbs_overflow", transfer_ring);
                return false;
            };
            let Some(completion) = find_control_completion(mem, completion_gpa) else {
                trace::ep0_reject_with_gpa("completion_trbs_invalid", completion_gpa);
                return false;
            };
            let posted = self.post_control_completion_events(
                mem,
                ControlEventRequest {
                    setup,
                    data_stage: None,
                    completion,
                    residual_length: 0,
                    transferred_length: 0,
                },
            );
            if posted {
                if let Some(value) = set_configuration {
                    self.usb_configuration = value;
                }
            }
            return posted;
        }
        if is_hid_set_report_request(setup_packet) {
            return self.process_ep0_data_out_control_transfer(mem, transfer_ring, setup);
        }
        self.process_ep0_data_in_control_transfer(mem, transfer_ring, setup)
    }

    fn process_ep0_data_out_control_transfer(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        transfer_ring: u64,
        setup: TransferTrb,
    ) -> bool {
        let Some(data_gpa) = transfer_ring.checked_add(DATA_STAGE_OFFSET) else {
            trace::ep0_reject_with_gpa("data_trb_overflow", transfer_ring);
            return false;
        };
        let Some(data) = read_transfer_trb(mem, data_gpa) else {
            trace::ep0_reject_with_gpa("data_trb_read_failed", data_gpa);
            return false;
        };
        trace_transfer_trb("data", data);
        let Some(completion_gpa) = transfer_ring.checked_add(STATUS_STAGE_OFFSET) else {
            trace::ep0_reject_with_gpa("completion_trbs_overflow", transfer_ring);
            return false;
        };
        let Some(completion) = find_control_completion(mem, completion_gpa) else {
            trace::ep0_reject_with_gpa("completion_trbs_invalid", completion_gpa);
            return false;
        };
        let setup_packet = parse_setup_packet(setup.parameter);
        let data_type = trb_type(data.control);
        if data_type != TRB_TYPE_DATA_STAGE {
            trace::ep0_reject_with_value("unexpected_data_trb_type", data_type);
            return false;
        }
        if data.control & TRB_DATA_STAGE_DIRECTION_IN != 0 {
            trace::ep0_reject("data_stage_not_out");
            return false;
        }
        let data_length = trb_transfer_length(data.status);
        if data_length != u32::from(setup_packet.length) {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        }
        let Ok(data_len) = usize::try_from(data_length) else {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        };
        if mem.read_bytes(data.parameter, data_len).is_none() {
            trace::ep0_reject_with_gpa("data_read_failed", data.parameter);
            return false;
        }
        self.post_control_completion_events(
            mem,
            ControlEventRequest {
                setup,
                data_stage: Some(data),
                completion,
                residual_length: 0,
                transferred_length: data_length,
            },
        )
    }

    fn process_ep0_data_in_control_transfer(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        transfer_ring: u64,
        setup: TransferTrb,
    ) -> bool {
        let Some(data_gpa) = transfer_ring.checked_add(DATA_STAGE_OFFSET) else {
            trace::ep0_reject_with_gpa("data_trb_overflow", transfer_ring);
            return false;
        };
        let Some(data) = read_transfer_trb(mem, data_gpa) else {
            trace::ep0_reject_with_gpa("data_trb_read_failed", data_gpa);
            return false;
        };
        trace_transfer_trb("data", data);
        let Some(completion_gpa) = transfer_ring.checked_add(STATUS_STAGE_OFFSET) else {
            trace::ep0_reject_with_gpa("completion_trbs_overflow", transfer_ring);
            return false;
        };
        let Some(completion) = find_control_completion(mem, completion_gpa) else {
            trace::ep0_reject_with_gpa("completion_trbs_invalid", completion_gpa);
            return false;
        };
        let setup_packet = parse_setup_packet(setup.parameter);
        let Some(data_in) = data_in_for_setup_packet(setup_packet, self.usb_configuration) else {
            trace::ep0_reject("unsupported_setup_packet");
            return false;
        };
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
        let Ok(max_data_length) = u32::try_from(data_in.len()) else {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        };
        if data_length == 0 {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        }
        if u32::from(setup_packet.length) != data_length {
            trace::ep0_reject_with_value("unexpected_setup_length", u32::from(setup_packet.length));
            return false;
        }
        let transfer_length = data_length.min(max_data_length);
        let residual_length = data_length - transfer_length;
        let Ok(write_length) = usize::try_from(transfer_length) else {
            trace::ep0_reject_with_value("unexpected_data_length", data_length);
            return false;
        };
        if !write_control_in_data(mem, data.parameter, data_in, write_length) {
            trace::ep0_reject_with_gpa("descriptor_write_failed", data.parameter);
            return false;
        }
        trace::ep0_descriptor_write_success(data.parameter, write_length);
        self.post_control_completion_events(
            mem,
            ControlEventRequest {
                setup,
                data_stage: Some(data),
                completion,
                residual_length,
                transferred_length: transfer_length,
            },
        )
    }
}

fn write_control_in_data(
    mem: &mut dyn GuestMemoryMut,
    gpa: u64,
    data: ControlInData,
    len: usize,
) -> bool {
    match data {
        ControlInData::Static(bytes) => mem.write_bytes(gpa, &bytes[..len]),
        ControlInData::Byte(value) => {
            let bytes = [value];
            mem.write_bytes(gpa, &bytes[..len])
        }
    }
}
