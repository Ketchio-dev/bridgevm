use crate::fwcfg::GuestMemoryMut;

use super::{
    interrupt_trb::{
        read_transfer_trb, transfer_event_control, trb_transfer_length, trb_type,
        COMPLETION_CODE_SHIFT, COMPLETION_CODE_SUCCESS, LINK_TRB_POINTER_MASK, TRB_CYCLE,
        TRB_LINK_TOGGLE_CYCLE, TRB_SIZE_BYTES, TRB_TYPE_LINK, TRB_TYPE_NORMAL,
    },
    setup_input_report::SetupInputReport,
    setup_input_report::{HID_BOOT_KEYBOARD_NO_KEY_REPORT, HID_BOOT_KEYBOARD_REPORT_LEN},
    XhciController,
};

const SLOT_ID: u32 = 1;
const ENDPOINT_ID_DCI3: u32 = 3;
const MAX_LINK_TRBS_PER_DOORBELL: usize = 8;
const MIN_REUSABLE_DCI3_RING_TRBS: u64 = 4;
const MIN_DELAYED_REUSABLE_DCI3_RING_TRBS: u64 = 2;

#[derive(Clone, Copy)]
enum Dci3RearmPolicy {
    AfterDoorbell,
    ReusableQueueDrain,
}

impl Dci3RearmPolicy {
    const fn minimum_consumed_trbs(self, two_entry_queue_rearm: bool) -> Option<u64> {
        match self {
            Self::AfterDoorbell => Some(1),
            Self::ReusableQueueDrain => Some(if two_entry_queue_rearm {
                MIN_DELAYED_REUSABLE_DCI3_RING_TRBS
            } else {
                MIN_REUSABLE_DCI3_RING_TRBS
            }),
        }
    }
}

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
                return false;
            }
            let Some(interrupt_transfer) = read_transfer_trb(mem, transfer_ring) else {
                if self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy) {
                    continue;
                }
                return false;
            };
            let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
                if self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy) {
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
                    let Some(next_dequeue) = interrupt_transfer.gpa.checked_add(TRB_SIZE_BYTES)
                    else {
                        return false;
                    };
                    let transfer_length = trb_transfer_length(interrupt_transfer.status);
                    let can_emit_queued_report = transfer_length >= HID_BOOT_KEYBOARD_REPORT_LEN;
                    let queued_report = self.boot_keyboard_report_queue.peek();
                    let report = if can_emit_queued_report {
                        queued_report
                            .map(SetupInputReport::bytes)
                            .unwrap_or(HID_BOOT_KEYBOARD_NO_KEY_REPORT)
                    } else {
                        HID_BOOT_KEYBOARD_NO_KEY_REPORT
                    };
                    let write_len = transfer_length.min(HID_BOOT_KEYBOARD_REPORT_LEN);
                    let Ok(write_len) = usize::try_from(write_len) else {
                        return false;
                    };
                    if write_len > 0
                        && !mem.write_bytes(interrupt_transfer.parameter, &report[..write_len])
                    {
                        return false;
                    }
                    let residual_length =
                        transfer_length.saturating_sub(HID_BOOT_KEYBOARD_REPORT_LEN);
                    let event_status =
                        (COMPLETION_CODE_SUCCESS << COMPLETION_CODE_SHIFT) | residual_length;
                    let event_control = transfer_event_control(SLOT_ID, ENDPOINT_ID_DCI3);
                    let posted =
                        self.post_event(mem, interrupt_transfer.gpa, event_status, event_control);
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
                    }
                    return posted;
                }
                _ => {
                    if self.rearm_slot1_dci3_to_ring_base_if_queued(mem, rearm_policy) {
                        continue;
                    }
                    return false;
                }
            }
        }
        false
    }

    fn rearm_slot1_dci3_to_ring_base_if_queued(
        &mut self,
        mem: &dyn GuestMemoryMut,
        rearm_policy: Dci3RearmPolicy,
    ) -> bool {
        if !self.has_queued_setup_input_report() || self.slot1_dci3_ring_base == 0 {
            return false;
        }
        let wrapped_base_rearm = matches!(rearm_policy, Dci3RearmPolicy::ReusableQueueDrain)
            && self.slot1_dci3_dequeue == self.slot1_dci3_ring_base
            && self.slot1_dci3_two_entry_queue_rearm;
        if self.slot1_dci3_dequeue == self.slot1_dci3_ring_base && !wrapped_base_rearm {
            if !matches!(rearm_policy, Dci3RearmPolicy::ReusableQueueDrain) {
                return false;
            }
            let Some(interrupt_transfer) = read_transfer_trb(mem, self.slot1_dci3_ring_base) else {
                return false;
            };
            let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE == expected_cycle {
                return false;
            }
            return match trb_type(interrupt_transfer.control) {
                TRB_TYPE_LINK | TRB_TYPE_NORMAL => {
                    self.slot1_dci3_dcs = interrupt_transfer.control & TRB_CYCLE != 0;
                    self.slot1_dci3_dequeue = self.slot1_dci3_ring_base;
                    true
                }
                _ => false,
            };
        }
        let Some(minimum_consumed_trbs) =
            rearm_policy.minimum_consumed_trbs(self.slot1_dci3_two_entry_queue_rearm)
        else {
            return false;
        };
        if !wrapped_base_rearm {
            let Some(consumed_bytes) = self
                .slot1_dci3_dequeue
                .checked_sub(self.slot1_dci3_ring_base)
            else {
                return false;
            };
            if consumed_bytes % TRB_SIZE_BYTES != 0
                || consumed_bytes / TRB_SIZE_BYTES < minimum_consumed_trbs
            {
                return false;
            }
        }
        let Some(interrupt_transfer) = read_transfer_trb(mem, self.slot1_dci3_ring_base) else {
            return false;
        };
        let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
        if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
            if !wrapped_base_rearm {
                return false;
            }
            self.slot1_dci3_dcs = interrupt_transfer.control & TRB_CYCLE != 0;
        }
        match trb_type(interrupt_transfer.control) {
            TRB_TYPE_LINK | TRB_TYPE_NORMAL => {
                self.slot1_dci3_dequeue = self.slot1_dci3_ring_base;
                true
            }
            _ => false,
        }
    }

    pub(super) fn arm_two_entry_dci3_queue_rearm_if_consumed(&mut self) {
        self.slot1_dci3_two_entry_queue_rearm = self.slot1_dci3_two_entry_queue_rearm
            || self.consumed_slot1_dci3_ring_trbs() >= MIN_DELAYED_REUSABLE_DCI3_RING_TRBS;
    }

    fn consumed_slot1_dci3_ring_trbs(&self) -> u64 {
        if self.slot1_dci3_ring_base == 0 {
            return 0;
        }
        let Some(consumed_bytes) = self
            .slot1_dci3_dequeue
            .checked_sub(self.slot1_dci3_ring_base)
        else {
            return 0;
        };
        if consumed_bytes % TRB_SIZE_BYTES != 0 {
            return 0;
        }
        consumed_bytes / TRB_SIZE_BYTES
    }
}
