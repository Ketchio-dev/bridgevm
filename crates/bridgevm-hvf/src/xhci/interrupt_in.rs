use crate::fwcfg::GuestMemoryMut;

use super::{
    dci3_rearm::{Dci3RearmPolicy, Dci3RearmResult},
    interrupt_trb::{
        read_chained_event_data, read_transfer_trb, trb_transfer_length, trb_type,
        InterruptTransferTrb, LINK_TRB_POINTER_MASK, TRB_CYCLE, TRB_LINK_TOGGLE_CYCLE,
        TRB_SIZE_BYTES, TRB_TYPE_LINK, TRB_TYPE_NORMAL,
    },
    setup_input_report::{HID_BOOT_KEYBOARD_NO_KEY_REPORT, HID_BOOT_KEYBOARD_REPORT_LEN},
    XhciController,
};

const ENDPOINT_ID_DCI3: u32 = 3;
const MAX_LINK_TRBS_PER_DOORBELL: usize = 8;

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
        if matches!(self.hid_endpoints[0].state, 2..=4) {
            return false;
        }
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
                if self.rearm_slot1_dci3_or_trace_blocked(mem, rearm_policy) {
                    continue;
                }
                return false;
            };
            let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
                trace_blocked_trb("cycle_mismatch", &interrupt_transfer, self.slot1_dci3_dcs);
                if self.rearm_slot1_dci3_or_trace_blocked(mem, rearm_policy) {
                    continue;
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
                    return self.complete_slot1_dci3_normal_td(
                        mem,
                        interrupt_transfer,
                        rearm_policy,
                    );
                }
                _ => {
                    trace_blocked_trb("unexpected_type", &interrupt_transfer, self.slot1_dci3_dcs);
                    if !self.rearm_slot1_dci3_or_trace_blocked(mem, rearm_policy) {
                        return false;
                    }
                }
            }
        }
        self.trace_dci3_drain_blocked("link_trb_limit", rearm_policy);
        false
    }

    /// Completes the Normal TD at the DCI3 dequeue pointer with the next
    /// queued boot-keyboard report. Returns whether its event was posted.
    fn complete_slot1_dci3_normal_td(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        normal: InterruptTransferTrb,
        rearm_policy: Dci3RearmPolicy,
    ) -> bool {
        self.hid_endpoints[0].pending = Some((normal.gpa, normal.status));
        // Windows chains the interrupt-IN Normal TRB to an Event Data TRB
        // carrying the URB cookie; the TD completes with one Event Data
        // completion instead of a per-TRB event.
        let event_data = read_chained_event_data(mem, &normal, self.slot1_dci3_dcs);
        let last_td_trb_gpa = event_data
            .as_ref()
            .map_or(normal.gpa, |event_data| event_data.trb.gpa);
        let Some(next_dequeue) = last_td_trb_gpa.checked_add(TRB_SIZE_BYTES) else {
            self.trace_dci3_drain_blocked("dequeue_overflow", rearm_policy);
            return false;
        };
        let Some(queued_report) = self.boot_keyboard_report_queue.peek() else {
            // A real HID interrupt-IN endpoint NAKs when it has no report to
            // send; the host controller then leaves the transfer descriptor
            // armed and retries at the polling interval without posting a
            // transfer event. Completing an idle poll with a synthetic no-key
            // report instead makes the guest ack and immediately re-ring the
            // doorbell, which livelocks the interrupter (one event and
            // interrupt per poll). Leave the Normal TD pending (no report
            // write, no event, no dequeue advance) until real input is queued
            // and drained.
            self.trace_dci3_drain_blocked("no_queued_input_report", rearm_policy);
            return false;
        };
        let can_emit_queued_report =
            trb_transfer_length(normal.status) >= HID_BOOT_KEYBOARD_REPORT_LEN;
        let report = if can_emit_queued_report {
            queued_report.bytes()
        } else {
            self.trace_dci3_drain_blocked("short_interrupt_in_buffer", rearm_policy);
            HID_BOOT_KEYBOARD_NO_KEY_REPORT
        };
        // Without Event Data a boot-keyboard TD completes Success even when short.
        let Some(fill) = self.fill_interrupt_in_td(
            mem,
            ENDPOINT_ID_DCI3,
            &normal,
            event_data.as_ref(),
            &report,
            false,
        ) else {
            self.trace_dci3_drain_blocked("write_failed", rearm_policy);
            return false;
        };
        if !fill.posted {
            self.trace_dci3_drain_blocked("event_post_failed", rearm_policy);
            return false;
        }
        self.hid_endpoints[0].pending = None;
        if can_emit_queued_report {
            self.record_setup_input_report_emitted(
                queued_report,
                report,
                normal.gpa,
                normal.parameter,
            );
            self.boot_keyboard_report_queue.pop_front();
            if !self.has_queued_setup_input_report() {
                self.slot1_dci3_two_entry_queue_rearm = false;
            }
        }
        self.slot1_dci3_dequeue = next_dequeue;
        if event_data.is_some_and(|event_data| event_data.toggles_cycle) {
            self.slot1_dci3_dcs = !self.slot1_dci3_dcs;
        }
        self.write_slot1_dci3_output_dequeue(mem);
        true
    }

    /// Re-arms DCI3 at its ring base when input is queued, else traces why
    /// not. Returns whether the ring walk goes on.
    fn rearm_slot1_dci3_or_trace_blocked(
        &mut self,
        mem: &dyn GuestMemoryMut,
        rearm_policy: Dci3RearmPolicy,
    ) -> bool {
        match self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy) {
            Dci3RearmResult::Rearmed => true,
            Dci3RearmResult::Refused(rearm_reason) => {
                self.trace_dci3_drain_blocked(rearm_reason, rearm_policy);
                false
            }
        }
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

    pub(super) fn reacquire_slot1_dci3_from_dequeue(
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

fn trace_blocked_trb(reason: &str, trb: &InterruptTransferTrb, expected_dcs: bool) {
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
