use crate::fwcfg::GuestMemoryMut;

use super::super::{
    trace,
    usb::{
        data_in_for_setup_packet, is_clear_endpoint_halt_request,
        is_hid_class_descriptor_get_request, is_hid_report_descriptor_get_request,
        is_hid_set_idle_request, is_hid_set_protocol_request, is_hid_set_report_request,
        is_supported_setup_packet, parse_setup_packet, set_configuration_value,
    },
    XhciController,
};
use super::completion::{find_control_completion, ControlEventRequest};
use super::control_data::write_control_in_data;
use super::control_setup::{control_setup_start, ControlSetupStart};
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
        let current_dequeue = self.slot1_ep0_dequeue;
        trace::ep0_handler_entered(current_dequeue);
        if current_dequeue == 0 {
            trace::ep0_reject("no_ep0_dequeue");
            return false;
        }
        let transfer_ring = match control_setup_start(mem, current_dequeue) {
            ControlSetupStart::Ready(gpa) => gpa,
            ControlSetupStart::ReadFailed(gpa) => {
                trace::ep0_reject_with_gpa("setup_trb_read_failed", gpa);
                return false;
            }
        };
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
        self.record_ep0_setup_packet(setup_packet);
        let set_configuration = set_configuration_value(setup_packet);
        let set_protocol = is_hid_set_protocol_request(setup_packet);
        let set_idle = is_hid_set_idle_request(setup_packet);
        let clear_endpoint_halt = is_clear_endpoint_halt_request(setup_packet);
        let set_report = is_hid_set_report_request(setup_packet);
        if !is_supported_setup_packet(setup_packet, self.usb_configuration) {
            self.record_unsupported_setup_packet(setup_packet);
        }
        if set_configuration.is_some() || set_protocol || set_idle || clear_endpoint_halt {
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
                if set_protocol {
                    self.record_hid_set_protocol(setup_packet.index, setup_packet.value);
                }
                if set_idle {
                    self.record_hid_set_idle(setup_packet.index);
                }
                if clear_endpoint_halt {
                    self.record_clear_endpoint_halt();
                }
            }
            return posted;
        }
        if set_report {
            let posted = self.process_ep0_data_out_control_transfer(mem, transfer_ring, setup);
            if posted {
                self.record_hid_set_report(setup_packet.index);
            }
            return posted;
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
        if !guest_range_readable(mem, data.parameter, data_len) {
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
        let hid_report_descriptor_get = is_hid_report_descriptor_get_request(setup_packet);
        let hid_class_descriptor_get = is_hid_class_descriptor_get_request(setup_packet);
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
        let posted = self.post_control_completion_events(
            mem,
            ControlEventRequest {
                setup,
                data_stage: Some(data),
                completion,
                residual_length,
                transferred_length: transfer_length,
            },
        );
        if posted && hid_report_descriptor_get {
            self.record_hid_report_descriptor_get(setup_packet.index, setup_packet.length);
        }
        if posted && hid_class_descriptor_get {
            self.record_hid_class_descriptor_get(setup_packet.index, setup_packet.length);
        }
        posted
    }
}

fn guest_range_readable(mem: &dyn GuestMemoryMut, mut gpa: u64, mut len: usize) -> bool {
    let mut scratch = [0u8; 256];
    while len != 0 {
        let chunk = len.min(scratch.len());
        if !mem.read_into(gpa, &mut scratch[..chunk]) {
            return false;
        }
        let Ok(chunk_u64) = u64::try_from(chunk) else {
            return false;
        };
        let Some(next_gpa) = gpa.checked_add(chunk_u64) else {
            return false;
        };
        gpa = next_gpa;
        len -= chunk;
    }
    true
}
