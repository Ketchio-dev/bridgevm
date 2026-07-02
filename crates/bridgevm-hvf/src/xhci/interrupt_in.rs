use crate::fwcfg::GuestMemoryMut;

use super::{
    dci3_rearm::{Dci3RearmPolicy, Dci3RearmResult},
    interrupt_trb::{
        read_transfer_trb, transfer_event_control, trb_interrupter_target, trb_transfer_length,
        trb_type, InterruptTransferTrb, COMPLETION_CODE_SHIFT, COMPLETION_CODE_SUCCESS,
        LINK_TRB_POINTER_MASK, TRB_CYCLE, TRB_LINK_TOGGLE_CYCLE, TRB_SIZE_BYTES, TRB_TYPE_LINK,
        TRB_TYPE_NORMAL,
    },
    setup_input_report::SetupInputReport,
    setup_input_report::{HID_BOOT_KEYBOARD_NO_KEY_REPORT, HID_BOOT_KEYBOARD_REPORT_LEN},
    XhciController,
};

const SLOT_ID: u32 = 1;
const ENDPOINT_ID_DCI3: u32 = 3;
const MAX_LINK_TRBS_PER_DOORBELL: usize = 8;
const TRB_CHAIN: u32 = 1 << 4;
const TRB_IOC: u32 = 1 << 5;
const TRB_TYPE_EVENT_DATA: u32 = 7;
const TRANSFER_EVENT_ED: u32 = 1 << 2;
/// QEMU oracle: a TD that moves fewer bytes than requested completes with
/// Short Packet, not Success.
const COMPLETION_CODE_SHORT_PACKET: u32 = 13;

impl XhciController {
    pub(crate) fn process_dci3_interrupt_in_transfer(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.process_dci3_interrupt_in_transfer_with_rearm(mem, Dci3RearmPolicy::ReusableQueueDrain)
    }

    pub(crate) fn process_queued_dci3_input(&mut self, mem: &mut dyn GuestMemoryMut) -> bool {
        self.process_dci3_interrupt_in_transfer_with_rearm(mem, Dci3RearmPolicy::ReusableQueueDrain)
    }

    pub(super) fn process_dci3_interrupt_in_transfer_after_doorbell(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.process_dci3_interrupt_in_transfer_with_rearm(mem, Dci3RearmPolicy::AfterDoorbell)
    }

    fn process_dci3_interrupt_in_transfer_with_rearm(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        rearm_policy: Dci3RearmPolicy,
    ) -> bool {
        for _ in 0..MAX_LINK_TRBS_PER_DOORBELL {
            let transfer_ring = self.slot1_dci3_dequeue;
            if transfer_ring == 0 {
                if self.reacquire_slot1_dci3_from_output_context_if_ready(mem) {
                    continue;
                }
                self.trace_dci3_drain_blocked("no_dci3_endpoint", rearm_policy);
                return false;
            }
            let Some(interrupt_transfer) = read_transfer_trb(mem, transfer_ring) else {
                let rearm_result = self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy);
                if rearm_result.is_rearmed() {
                    continue;
                }
                if let Dci3RearmResult::Refused(rearm_reason) = rearm_result {
                    self.trace_dci3_drain_blocked(rearm_reason, rearm_policy);
                }
                return false;
            };
            let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
                trace_blocked_trb("cycle_mismatch", &interrupt_transfer, self.slot1_dci3_dcs);
                let rearm_result = self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy);
                if rearm_result.is_rearmed() {
                    continue;
                }
                if let Dci3RearmResult::Refused(rearm_reason) = rearm_result {
                    self.trace_dci3_drain_blocked(rearm_reason, rearm_policy);
                }
                return false;
            }
            match trb_type(interrupt_transfer.control) {
                TRB_TYPE_LINK => {
                    let next_dequeue = interrupt_transfer.parameter & LINK_TRB_POINTER_MASK;
                    let wraps_to_ring_base =
                        next_dequeue == self.slot1_dci3_ring_base && self.slot1_dci3_ring_base != 0;
                    self.slot1_dci3_dequeue = next_dequeue;
                    if interrupt_transfer.control & TRB_LINK_TOGGLE_CYCLE != 0 {
                        self.slot1_dci3_dcs = !self.slot1_dci3_dcs;
                        if wraps_to_ring_base {
                            self.slot1_dci3_two_entry_queue_rearm = true;
                        }
                    }
                    self.write_slot1_dci3_output_dequeue(mem);
                }
                TRB_TYPE_NORMAL => {
                    // Windows chains the interrupt-IN Normal TRB to an Event
                    // Data TRB carrying the URB cookie; the TD completes with
                    // one Event Data completion instead of a per-TRB event.
                    let event_data =
                        read_chained_event_data(mem, &interrupt_transfer, self.slot1_dci3_dcs);
                    let last_td_trb_gpa = event_data
                        .as_ref()
                        .map(|event_data| event_data.gpa)
                        .unwrap_or(interrupt_transfer.gpa);
                    let Some(next_dequeue) = last_td_trb_gpa.checked_add(TRB_SIZE_BYTES) else {
                        self.trace_dci3_drain_blocked("dequeue_overflow", rearm_policy);
                        return false;
                    };
                    let transfer_length = trb_transfer_length(interrupt_transfer.status);
                    let can_emit_queued_report = transfer_length >= HID_BOOT_KEYBOARD_REPORT_LEN;
                    let queued_report = self.boot_keyboard_report_queue.peek();
                    if queued_report.is_some() && !can_emit_queued_report {
                        self.trace_dci3_drain_blocked("short_interrupt_in_buffer", rearm_policy);
                    }
                    let report = if can_emit_queued_report {
                        queued_report
                            .map(SetupInputReport::bytes)
                            .unwrap_or(HID_BOOT_KEYBOARD_NO_KEY_REPORT)
                    } else {
                        HID_BOOT_KEYBOARD_NO_KEY_REPORT
                    };
                    let write_len = transfer_length.min(HID_BOOT_KEYBOARD_REPORT_LEN);
                    let Ok(write_len) = usize::try_from(write_len) else {
                        self.trace_dci3_drain_blocked("short_interrupt_in_buffer", rearm_policy);
                        return false;
                    };
                    if write_len > 0
                        && !mem.write_bytes(interrupt_transfer.parameter, &report[..write_len])
                    {
                        self.trace_dci3_drain_blocked("write_failed", rearm_policy);
                        return false;
                    }
                    let written_length = write_len as u32;
                    let completion_code = if written_length < transfer_length {
                        COMPLETION_CODE_SHORT_PACKET
                    } else {
                        COMPLETION_CODE_SUCCESS
                    };
                    let event_control = transfer_event_control(SLOT_ID, ENDPOINT_ID_DCI3);
                    let posted = match event_data.as_ref() {
                        Some(event_data) => self.post_event_to_interrupter(
                            mem,
                            trb_interrupter_target(event_data.status),
                            event_data.parameter,
                            (completion_code << COMPLETION_CODE_SHIFT) | written_length,
                            event_control | TRANSFER_EVENT_ED,
                        ),
                        None => {
                            let residual_length =
                                transfer_length.saturating_sub(HID_BOOT_KEYBOARD_REPORT_LEN);
                            self.post_event_to_interrupter(
                                mem,
                                trb_interrupter_target(interrupt_transfer.status),
                                interrupt_transfer.gpa,
                                (COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT)
                                    | residual_length,
                                event_control,
                            )
                        }
                    };
                    if posted {
                        if can_emit_queued_report {
                            if let Some(queued_report) = queued_report {
                                self.record_setup_input_report_emitted(
                                    queued_report,
                                    report,
                                    interrupt_transfer.gpa,
                                    interrupt_transfer.parameter,
                                );
                                self.boot_keyboard_report_queue.pop_front();
                                if !self.has_queued_setup_input_report() {
                                    self.slot1_dci3_two_entry_queue_rearm = false;
                                }
                            }
                        }
                        self.slot1_dci3_dequeue = next_dequeue;
                        self.write_slot1_dci3_output_dequeue(mem);
                    } else {
                        self.trace_dci3_drain_blocked("event_post_failed", rearm_policy);
                    }
                    return posted;
                }
                _ => {
                    trace_blocked_trb("unexpected_type", &interrupt_transfer, self.slot1_dci3_dcs);
                    let rearm_result =
                        self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy);
                    if rearm_result.is_rearmed() {
                        continue;
                    }
                    if let Dci3RearmResult::Refused(rearm_reason) = rearm_result {
                        self.trace_dci3_drain_blocked(rearm_reason, rearm_policy);
                    }
                    return false;
                }
            }
        }
        self.trace_dci3_drain_blocked("link_trb_limit", rearm_policy);
        false
    }

    fn reacquire_slot1_dci3_from_output_context_if_ready(
        &mut self,
        mem: &dyn GuestMemoryMut,
    ) -> bool {
        let Some((dequeue, dcs)) = self.slot1_dci3_output_dequeue_state(mem).or_else(|| {
            (self.slot1_dci3_last_reusable && self.slot1_dci3_last_dequeue != 0)
                .then_some((self.slot1_dci3_last_dequeue, self.slot1_dci3_last_dcs))
        }) else {
            return false;
        };
        self.reacquire_slot1_dci3_from_dequeue(mem, dequeue, dcs)
    }

    fn reacquire_slot1_dci3_from_dequeue(
        &mut self,
        mem: &dyn GuestMemoryMut,
        dequeue: u64,
        dcs: bool,
    ) -> bool {
        let Some(interrupt_transfer) = read_transfer_trb(mem, dequeue) else {
            return false;
        };
        let expected_cycle = if dcs { TRB_CYCLE } else { 0 };
        if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
            return false;
        }
        match trb_type(interrupt_transfer.control) {
            TRB_TYPE_LINK | TRB_TYPE_NORMAL => {
                self.slot1_dci3_dequeue = dequeue;
                self.slot1_dci3_ring_base = dequeue;
                self.slot1_dci3_dcs = dcs;
                self.slot1_dci3_two_entry_queue_rearm = false;
                true
            }
            _ => false,
        }
    }
}

fn read_chained_event_data(
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

fn trace_blocked_trb(
    reason: &str,
    trb: &super::interrupt_trb::InterruptTransferTrb,
    expected_dcs: bool,
) {
    if super::trace::bringup_enabled() {
        println!(
            "XHCI DCI3 blocked_trb reason={reason} gpa={gpa:#x} parameter={parameter:#x} status={status:#010x} control={control:#010x} expected_dcs={expected_dcs}",
            gpa = trb.gpa,
            parameter = trb.parameter,
            status = trb.status,
            control = trb.control,
        );
    }
}
