use crate::fwcfg::GuestMemoryMut;

use super::super::{trace, XhciController};
use super::trb::{
    read_transfer_trb, trace_transfer_trb, trb_interrupter_target, trb_type, TransferTrb,
};

const TRB_SIZE_BYTES: u64 = 16;
const TRB_TYPE_SHIFT: u32 = 10;
const TRB_TYPE_STATUS_STAGE: u32 = 4;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRB_TYPE_TRANSFER_EVENT: u32 = 32;
const TRB_IOC: u32 = 1 << 5;
const TRB_CHAIN: u32 = 1 << 4;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
const COMPLETION_CODE_SUCCESS: u32 = 1;
/// QEMU oracle: a TD that moves fewer bytes than requested completes with
/// Short Packet, not Success (xhci_xfer_report rewrites the ccode).
const COMPLETION_CODE_SHORT_PACKET: u32 = 13;
const COMPLETION_CODE_SHIFT: u32 = 24;
const SLOT_ID: u32 = 1;
const ENDPOINT_ID_EP0: u32 = 1;
const EVENT_ENDPOINT_ID_SHIFT: u32 = 16;
const EVENT_SLOT_ID_SHIFT: u32 = 24;

#[derive(Clone, Copy)]
pub(super) struct ControlCompletion {
    status_stage: TransferTrb,
    event_data: Option<TransferTrb>,
    /// Windows bootmgr chains the Status Stage TRB to a trailing Event Data
    /// TRB that requests the status TD's own completion event.
    trailing_event_data: Option<TransferTrb>,
}

pub(super) struct ControlEventRequest {
    pub(super) setup: TransferTrb,
    pub(super) data_stage: Option<TransferTrb>,
    pub(super) completion: ControlCompletion,
    pub(super) residual_length: u32,
    /// Bytes actually transferred by the TD; Event Data completions report
    /// this as the EDTLA transfer length.
    pub(super) transferred_length: u32,
}

impl XhciController {
    pub(super) fn post_control_completion_events(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        request: ControlEventRequest,
    ) -> bool {
        let last_td_trb_gpa = request
            .completion
            .trailing_event_data
            .map(|trailing| trailing.gpa)
            .unwrap_or(request.completion.status_stage.gpa);
        let Some(next_dequeue) = last_td_trb_gpa.checked_add(TRB_SIZE_BYTES) else {
            trace::ep0_reject_with_gpa("status_stage_next_overflow", last_td_trb_gpa);
            return false;
        };
        let event_control_base = transfer_event_control(SLOT_ID, ENDPOINT_ID_EP0);
        let data_td_completion_code = if request.residual_length > 0 {
            COMPLETION_CODE_SHORT_PACKET
        } else {
            COMPLETION_CODE_SUCCESS
        };
        let posted = {
            let mut post_completion_event = |event_parameter: u64,
                                             completion_code: u32,
                                             transfer_length: u32,
                                             event_flags: u32,
                                             interrupter: usize|
             -> bool {
                let event_status = (completion_code << COMPLETION_CODE_SHIFT) | transfer_length;
                let event_control = event_control_base | event_flags;
                trace::ep0_post_event_request(event_parameter, event_status, event_control);
                let posted = self.post_event_to_interrupter(
                    mem,
                    interrupter,
                    event_parameter,
                    event_status,
                    event_control,
                );
                trace::ep0_post_event_result(posted);
                posted
            };
            let data_td_posted = match request.completion.event_data {
                Some(event_data) => post_completion_event(
                    event_data.parameter,
                    data_td_completion_code,
                    request.transferred_length,
                    TRANSFER_EVENT_ED,
                    trb_interrupter_target(event_data.status),
                ),
                None => {
                    if !post_completion_event(
                        request.setup.gpa,
                        COMPLETION_CODE_SUCCESS,
                        0,
                        0,
                        trb_interrupter_target(request.setup.status),
                    ) {
                        return false;
                    }
                    if let Some(data_stage) = request.data_stage {
                        if !post_completion_event(
                            data_stage.gpa,
                            data_td_completion_code,
                            request.residual_length,
                            0,
                            trb_interrupter_target(data_stage.status),
                        ) {
                            return false;
                        }
                    }
                    post_completion_event(
                        request.completion.status_stage.gpa,
                        COMPLETION_CODE_SUCCESS,
                        0,
                        0,
                        trb_interrupter_target(request.completion.status_stage.status),
                    )
                }
            };
            match (data_td_posted, request.completion.trailing_event_data) {
                (true, Some(trailing)) => post_completion_event(
                    trailing.parameter,
                    COMPLETION_CODE_SUCCESS,
                    0,
                    TRANSFER_EVENT_ED,
                    trb_interrupter_target(trailing.status),
                ),
                (posted, _) => posted,
            }
        };
        if posted {
            self.slot1_ep0_dequeue = next_dequeue;
            self.write_slot1_ep0_output_dequeue(mem);
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
            let trailing_event_data = find_trailing_event_data(mem, first)?;
            Some(ControlCompletion {
                status_stage: first,
                event_data: None,
                trailing_event_data,
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
                TRB_TYPE_STATUS_STAGE => {
                    let trailing_event_data = find_trailing_event_data(mem, second)?;
                    Some(ControlCompletion {
                        status_stage: second,
                        event_data: (first.control & TRB_IOC != 0).then_some(first),
                        trailing_event_data,
                    })
                }
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

fn find_trailing_event_data(
    mem: &dyn GuestMemoryMut,
    status_stage: TransferTrb,
) -> Option<Option<TransferTrb>> {
    if status_stage.control & TRB_CHAIN == 0 {
        return Some(None);
    }
    let Some(trailing_gpa) = status_stage.gpa.checked_add(TRB_SIZE_BYTES) else {
        trace::ep0_reject_with_gpa("status_chain_next_overflow", status_stage.gpa);
        return None;
    };
    let trailing = read_transfer_trb(mem, trailing_gpa)?;
    if trb_type(trailing.control) != TRB_TYPE_EVENT_DATA {
        trace::ep0_reject_with_value("status_chain_not_event_data", trb_type(trailing.control));
        return None;
    }
    trace_transfer_trb("event_data", trailing);
    Some((trailing.control & TRB_IOC != 0).then_some(trailing))
}

fn transfer_event_control(slot_id: u32, endpoint_id: u32) -> u32 {
    (slot_id << EVENT_SLOT_ID_SHIFT)
        | (endpoint_id << EVENT_ENDPOINT_ID_SHIFT)
        | (TRB_TYPE_TRANSFER_EVENT << TRB_TYPE_SHIFT)
}
