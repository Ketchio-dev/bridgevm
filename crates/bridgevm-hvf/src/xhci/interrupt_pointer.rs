use super::{
    interrupt_trb::{
        read_chained_event_data, read_transfer_trb, trb_transfer_length, trb_type,
        InterruptTransferTrb, LINK_TRB_POINTER_MASK, TRB_CYCLE, TRB_LINK_TOGGLE_CYCLE,
        TRB_SIZE_BYTES, TRB_TYPE_LINK, TRB_TYPE_NORMAL,
    },
    pointer_input_report::HID_ABSOLUTE_POINTER_REPORT_LEN,
    XhciController,
};
use crate::fwcfg::GuestMemoryMut;

const ENDPOINT_ID_DCI5: u32 = 5;
const MAX_LINK_TRBS_PER_DOORBELL: usize = 8;
const DCI5_DRAIN_POLICY_AFTER_DOORBELL: &str = "after_doorbell";
const DCI5_DRAIN_POLICY_QUEUED_POINTER: &str = "queued_pointer_drain";
impl XhciController {
    pub(super) fn process_dci5_interrupt_in_transfer_after_doorbell(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        // Host reports leave only through VirtPlatform's pacing path.
        if self.has_queued_pointer_input_report() {
            return false;
        }
        self.process_dci5_interrupt_in_transfer(mem, DCI5_DRAIN_POLICY_AFTER_DOORBELL)
    }
    pub(crate) fn process_queued_dci5_pointer_input(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
    ) -> bool {
        self.process_dci5_interrupt_in_transfer(mem, DCI5_DRAIN_POLICY_QUEUED_POINTER)
    }
    fn process_dci5_interrupt_in_transfer(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        policy: &'static str,
    ) -> bool {
        if matches!(self.hid_endpoints[1].state, 2..=4) {
            return false;
        }
        for _ in 0..MAX_LINK_TRBS_PER_DOORBELL {
            let transfer_ring = self.slot1_dci5_dequeue;
            if transfer_ring == 0 {
                if self.reacquire_slot1_dci5_from_output_context(mem) {
                    continue;
                }
                self.trace_dci5_drain_blocked("no_dci5_endpoint", policy, None);
                return false;
            }
            let Some(interrupt_transfer) = read_transfer_trb(mem, transfer_ring) else {
                self.trace_dci5_drain_blocked("trb_unreadable", policy, None);
                return false;
            };
            if is_empty_transfer_trb(&interrupt_transfer) {
                self.trace_dci5_drain_blocked(
                    "empty_transfer_ring",
                    policy,
                    Some(&interrupt_transfer),
                );
                return false;
            }
            let expected_cycle = if self.slot1_dci5_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
                if self.rearm_slot1_dci5_to_ring_base_if_queued(mem, policy) {
                    continue;
                }
                self.trace_dci5_drain_blocked("cycle_mismatch", policy, Some(&interrupt_transfer));
                return false;
            }
            match trb_type(interrupt_transfer.control) {
                TRB_TYPE_LINK => {
                    self.slot1_dci5_dequeue = interrupt_transfer.parameter & LINK_TRB_POINTER_MASK;
                    if interrupt_transfer.control & TRB_LINK_TOGGLE_CYCLE != 0 {
                        self.slot1_dci5_dcs = !self.slot1_dci5_dcs;
                    }
                    self.write_slot1_dci5_output_dequeue(mem);
                }
                TRB_TYPE_NORMAL => {
                    return self.complete_slot1_dci5_normal_td(mem, interrupt_transfer, policy);
                }
                _ => {
                    self.trace_dci5_drain_blocked(
                        "unexpected_type",
                        policy,
                        Some(&interrupt_transfer),
                    );
                    return false;
                }
            }
        }
        self.trace_dci5_drain_blocked("link_trb_limit", policy, None);
        false
    }
    /// Completes the Normal TD at the DCI5 dequeue pointer with the next
    /// queued pointer report. Returns whether its event was posted.
    fn complete_slot1_dci5_normal_td(
        &mut self,
        mem: &mut dyn GuestMemoryMut,
        normal: InterruptTransferTrb,
        policy: &'static str,
    ) -> bool {
        self.hid_endpoints[1].pending = Some((normal.gpa, normal.status));
        let event_data = read_chained_event_data(mem, &normal, self.slot1_dci5_dcs);
        let last_td_trb_gpa = event_data
            .as_ref()
            .map_or(normal.gpa, |event_data| event_data.trb.gpa);
        let Some(next_dequeue) = last_td_trb_gpa.checked_add(TRB_SIZE_BYTES) else {
            self.trace_dci5_drain_blocked("dequeue_overflow", policy, Some(&normal));
            return false;
        };
        let Some(queued_report) = self.pointer_input_report_queue.peek() else {
            // NAK idle polls like the boot keyboard: completing one
            // synthesizes a report the guest immediately acks and
            // re-polls, livelocking the interrupter. Leave the TD
            // pending until real pointer input is queued.
            self.trace_dci5_drain_blocked("no_queued_input_report", policy, Some(&normal));
            return false;
        };
        let transfer_length = trb_transfer_length(normal.status);
        let can_emit_queued_report = transfer_length >= HID_ABSOLUTE_POINTER_REPORT_LEN;
        let report = if can_emit_queued_report {
            queued_report.bytes()
        } else {
            self.trace_dci5_drain_blocked("short_interrupt_in_buffer", policy, Some(&normal));
            self.pointer_input_report_queue.idle_report()
        };
        // Without Event Data a short pointer TD completes Short Packet.
        let Some(fill) = self.fill_interrupt_in_td(
            mem,
            ENDPOINT_ID_DCI5,
            &normal,
            event_data.as_ref(),
            &report,
            true,
        ) else {
            self.trace_dci5_drain_blocked("write_failed", policy, Some(&normal));
            return false;
        };
        if !fill.posted {
            self.trace_dci5_drain_blocked("event_post_failed", policy, Some(&normal));
            return false;
        }
        self.hid_endpoints[1].pending = None;
        self.slot1_dci5_last_drain_blocked = None;
        if can_emit_queued_report {
            #[rustfmt::skip]
            let trace = super::trace_dci5_emission::trace_context(queued_report, &normal, last_td_trb_gpa, transfer_length, fill.written_length, fill.completion_code);
            self.trace_dci5_report_emitted(trace);
            self.record_pointer_input_report_emitted(queued_report);
            self.pointer_input_report_queue.pop_front();
        }
        self.slot1_dci5_dequeue = next_dequeue;
        if event_data.is_some_and(|event_data| event_data.toggles_cycle) {
            self.slot1_dci5_dcs = !self.slot1_dci5_dcs;
        }
        self.write_slot1_dci5_output_dequeue(mem);
        true
    }

    fn trace_dci5_drain_blocked(
        &mut self,
        reason: &'static str,
        policy: &'static str,
        trb: Option<&InterruptTransferTrb>,
    ) {
        if self.has_queued_pointer_input_report() {
            let trace = super::trace::Dci5DrainBlockedTrace {
                reason,
                policy,
                dequeue: self.slot1_dci5_dequeue,
                ring_base: self.slot1_dci5_ring_base,
                dcs: self.slot1_dci5_dcs,
                trb_gpa: trb.map(|trb| trb.gpa),
                trb_type: trb.map(|trb| trb_type(trb.control)),
                trb_cycle: trb.map(|trb| trb.control & TRB_CYCLE != 0),
                trb_parameter: trb.map(|trb| trb.parameter),
                trb_status: trb.map(|trb| trb.status),
                trb_control: trb.map(|trb| trb.control),
                queued_reports: self.pointer_input_report_stats.queued_reports,
                emitted_move_reports: self.pointer_input_report_stats.emitted_move_reports,
                emitted_button_reports: self.pointer_input_report_stats.emitted_button_reports,
                emitted_release_reports: self.pointer_input_report_stats.emitted_release_reports,
            };
            if self.slot1_dci5_last_drain_blocked == Some(trace) {
                return;
            }
            self.slot1_dci5_last_drain_blocked = Some(trace);
            super::trace::dci5_drain_blocked(trace);
        }
    }

    fn rearm_slot1_dci5_to_ring_base_if_queued(
        &mut self,
        mem: &dyn GuestMemoryMut,
        policy: &'static str,
    ) -> bool {
        if policy != DCI5_DRAIN_POLICY_QUEUED_POINTER
            || !self.has_queued_pointer_input_report()
            || self.slot1_dci5_ring_base == 0
            || self.slot1_dci5_dequeue != self.slot1_dci5_ring_base
        {
            return false;
        }
        let Some(interrupt_transfer) = read_transfer_trb(mem, self.slot1_dci5_ring_base) else {
            return false;
        };
        if !matches!(
            trb_type(interrupt_transfer.control),
            TRB_TYPE_LINK | TRB_TYPE_NORMAL
        ) {
            return false;
        }
        self.slot1_dci5_dcs = interrupt_transfer.control & TRB_CYCLE != 0;
        self.slot1_dci5_dequeue = self.slot1_dci5_ring_base;
        true
    }

    fn reacquire_slot1_dci5_from_output_context(&mut self, mem: &dyn GuestMemoryMut) -> bool {
        let Some((dequeue, dcs)) = self.slot1_dci5_output_dequeue_state(mem) else {
            return false;
        };
        self.slot1_dci5_dequeue = dequeue;
        self.slot1_dci5_ring_base = dequeue;
        self.slot1_dci5_dcs = dcs;
        self.slot1_dci5_last_drain_blocked = None;
        true
    }
}

fn is_empty_transfer_trb(trb: &InterruptTransferTrb) -> bool {
    trb.parameter == 0 && trb.status == 0 && trb.control == 0
}
