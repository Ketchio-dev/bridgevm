use crate::fwcfg::GuestMemoryMut;

use super::super::{trace, XhciController};
use super::trb::{read_transfer_trb, trace_transfer_trb, trb_type, TransferTrb};

const TRB_SIZE_BYTES: u64 = 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_IOC: u32 = 1 << 5;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
const COMPLETION_CODE_SUCCESS: u32 = 1;
const COMPLETION_CODE_SHIFT: u32 = 24;
const SLOT_ID: u32 = 1;
const ENDPOINT_ID_EP0: u32 = 1;
const EVENT_ENDPOINT_ID_SHIFT: u32 = 16;
const EVENT_SLOT_ID_SHIFT: u32 = 24;

#[derive(Clone, Copy)]
pub(super) struct ControlCompletion {
    status_stage: TransferTrb,
    event_data: Option<TransferTrb>,
}

pub(super) struct ControlEventRequest {
    pub(super) setup: TransferTrb,
    pub(super) data_stage: Option<TransferTrb>,
    pub(super) completion: ControlCompletion,
    pub(super) residual_length: u32,
}

impl XhciController {
    pub(super) fn post_control_completion_events(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        request: ControlEventRequest,
    ) -> bool {
        let Some(next_dequeue) = request
            .completion
            .status_stage
            .gpa
            .checked_add(TRB_SIZE_BYTES)
        else {
            trace::ep0_reject_with_gpa(
                "status_stage_next_overflow",
                request.completion.status_stage.gpa,
            );
            return false;
        };
        let event_control_base = transfer_event_control(SLOT_ID, ENDPOINT_ID_EP0);
        let start_event_status = COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT;
        trace::ep0_post_event_request(request.setup.gpa, start_event_status, event_control_base);
        let start_posted = self.post_event(
            mem,
            request.setup.gpa,
            start_event_status,
            event_control_base,
        );
        trace::ep0_post_event_result(start_posted);
        if !start_posted {
            return false;
        }
        let posted = {
            let mut post_completion_event =
                |event_parameter: u64, residual_length: u32, event_flags: u32| -> bool {
                    let event_status =
                        (COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT) | residual_length;
                    let event_control = event_control_base | event_flags;
                    trace::ep0_post_event_request(event_parameter, event_status, event_control);
                    let posted = self.post_event(mem, event_parameter, event_status, event_control);
                    trace::ep0_post_event_result(posted);
                    posted
                };
            match request.completion.event_data {
                Some(event_data) => post_completion_event(
                    event_data.parameter,
                    request.residual_length,
                    TRANSFER_EVENT_ED,
                ),
                None => {
                    if let Some(data_stage) = request.data_stage {
                        if !post_completion_event(data_stage.gpa, request.residual_length, 0) {
                            return false;
                        }
                    }
                    post_completion_event(request.completion.status_stage.gpa, 0, 0)
                }
            }
        };
        if posted {
            self.slot1_ep0_dequeue = next_dequeue;
        }
        posted
    }
}

pub(super) fn find_control_completion(
    mem: &dyn GuestMemoryMut,
    first_gpa: u64,
) -> Option<ControlCompletion> {
    let first = read_transfer_trb(mem, first_gpa)?;
    match trb_type(first.control) {
        TRB_TYPE_STATUS_STAGE => {
            trace_transfer_trb("status", first);
            Some(ControlCompletion {
                status_stage: first,
                event_data: None,
            })
        }
        TRB_TYPE_EVENT_DATA => {
            trace_transfer_trb("event_data", first);
            let Some(second_gpa) = first_gpa.checked_add(TRB_SIZE_BYTES) else {
                trace::ep0_reject_with_gpa("completion_second_overflow", first_gpa);
                return None;
            };
            let second = read_transfer_trb(mem, second_gpa)?;
            trace_transfer_trb("status", second);
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

fn transfer_event_control(slot_id: u32, endpoint_id: u32) -> u32 {
    (slot_id << EVENT_SLOT_ID_SHIFT)
        | (endpoint_id << EVENT_ENDPOINT_ID_SHIFT)
        | (TRB_TYPE_TRANSFER_EVENT << TRB_TYPE_SHIFT)
}
