use crate::fwcfg::GuestMemoryMut;

use super::{
    interrupt_trb::{
        read_transfer_trb, trb_type, TRB_CYCLE, TRB_SIZE_BYTES, TRB_TYPE_LINK, TRB_TYPE_NORMAL,
    },
    trace, XhciController,
};

const MIN_REUSABLE_DCI3_RING_TRBS: u64 = 4;
const MIN_DELAYED_REUSABLE_DCI3_RING_TRBS: u64 = 2;

#[derive(Clone, Copy)]
pub(super) enum Dci3RearmPolicy {
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

    const fn name(self) -> &'static str {
        match self {
            Self::AfterDoorbell => "after_doorbell",
            Self::ReusableQueueDrain => "reusable_queue_drain",
        }
    }
}

pub(super) enum Dci3RearmResult {
    Rearmed,
    Refused(&'static str),
}

impl Dci3RearmResult {
    pub(super) const fn is_rearmed(&self) -> bool {
        matches!(self, Self::Rearmed)
    }
}

impl XhciController {
    pub(super) fn rearm_slot1_dci3_to_ring_base_if_queued(
        &mut self,
        mem: &dyn GuestMemoryMut,
        rearm_policy: Dci3RearmPolicy,
    ) -> Dci3RearmResult {
        if !self.has_queued_setup_input_report() {
            return Dci3RearmResult::Refused("rearm_refused_no_queued_reports");
        }
        if self.slot1_dci3_ring_base == 0 {
            return Dci3RearmResult::Refused("rearm_refused_no_dci3_endpoint");
        }
        let wrapped_base_rearm = matches!(rearm_policy, Dci3RearmPolicy::ReusableQueueDrain)
            && self.slot1_dci3_dequeue == self.slot1_dci3_ring_base
            && self.slot1_dci3_two_entry_queue_rearm;
        if self.slot1_dci3_dequeue == self.slot1_dci3_ring_base && !wrapped_base_rearm {
            if !matches!(rearm_policy, Dci3RearmPolicy::ReusableQueueDrain) {
                return Dci3RearmResult::Refused(
                    "rearm_refused_policy_after_doorbell_at_ring_base",
                );
            }
            let Some(interrupt_transfer) = read_transfer_trb(mem, self.slot1_dci3_ring_base) else {
                return Dci3RearmResult::Refused("rearm_refused_ring_base_trb_unreadable");
            };
            let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
            if interrupt_transfer.control & TRB_CYCLE == expected_cycle {
                return Dci3RearmResult::Refused(
                    "rearm_refused_base_cycle_ready_without_two_entry",
                );
            }
            return match trb_type(interrupt_transfer.control) {
                TRB_TYPE_LINK | TRB_TYPE_NORMAL => {
                    self.slot1_dci3_dcs = interrupt_transfer.control & TRB_CYCLE != 0;
                    self.slot1_dci3_dequeue = self.slot1_dci3_ring_base;
                    Dci3RearmResult::Rearmed
                }
                _ => self
                    .rearm_slot1_dci3_to_last_dequeue_if_queued(mem, rearm_policy)
                    .unwrap_or(Dci3RearmResult::Refused(
                        "rearm_refused_unsupported_ring_base_trb_type",
                    )),
            };
        }
        let Some(minimum_consumed_trbs) =
            rearm_policy.minimum_consumed_trbs(self.slot1_dci3_two_entry_queue_rearm)
        else {
            return Dci3RearmResult::Refused("rearm_refused_policy");
        };
        if !wrapped_base_rearm {
            let Some(consumed_bytes) = self
                .slot1_dci3_dequeue
                .checked_sub(self.slot1_dci3_ring_base)
            else {
                return Dci3RearmResult::Refused("rearm_refused_dequeue_before_ring_base");
            };
            if consumed_bytes % TRB_SIZE_BYTES != 0
                || consumed_bytes / TRB_SIZE_BYTES < minimum_consumed_trbs
            {
                return Dci3RearmResult::Refused("rearm_refused_consumed_trbs_below_min");
            }
        }
        let Some(interrupt_transfer) = read_transfer_trb(mem, self.slot1_dci3_ring_base) else {
            return Dci3RearmResult::Refused("rearm_refused_ring_base_trb_unreadable");
        };
        let expected_cycle = if self.slot1_dci3_dcs { TRB_CYCLE } else { 0 };
        if interrupt_transfer.control & TRB_CYCLE != expected_cycle {
            let reusable_two_entry_cycle_resync =
                matches!(rearm_policy, Dci3RearmPolicy::ReusableQueueDrain)
                    && self.slot1_dci3_two_entry_queue_rearm;
            if !wrapped_base_rearm && !reusable_two_entry_cycle_resync {
                return Dci3RearmResult::Refused("rearm_refused_cycle_mismatch");
            }
            self.slot1_dci3_dcs = interrupt_transfer.control & TRB_CYCLE != 0;
        }
        match trb_type(interrupt_transfer.control) {
            TRB_TYPE_LINK | TRB_TYPE_NORMAL => {
                self.slot1_dci3_dequeue = self.slot1_dci3_ring_base;
                Dci3RearmResult::Rearmed
            }
            _ => self
                .rearm_slot1_dci3_to_last_dequeue_if_queued(mem, rearm_policy)
                .unwrap_or(Dci3RearmResult::Refused(
                    "rearm_refused_unsupported_ring_base_trb_type",
                )),
        }
    }

    fn rearm_slot1_dci3_to_last_dequeue_if_queued(
        &mut self,
        mem: &dyn GuestMemoryMut,
        rearm_policy: Dci3RearmPolicy,
    ) -> Option<Dci3RearmResult> {
        if !matches!(rearm_policy, Dci3RearmPolicy::ReusableQueueDrain)
            || !self.slot1_dci3_two_entry_queue_rearm
            || !self.slot1_dci3_last_reusable
            || self.slot1_dci3_last_dequeue == 0
            || self.slot1_dci3_last_dequeue == self.slot1_dci3_ring_base
            || self.slot1_dci3_last_ring_base != self.slot1_dci3_ring_base
        {
            return None;
        }

        self.reacquire_slot1_dci3_from_dequeue(
            mem,
            self.slot1_dci3_last_dequeue,
            self.slot1_dci3_last_dcs,
        )
        .then_some(Dci3RearmResult::Rearmed)
    }

    pub(super) fn trace_dci3_drain_blocked(
        &self,
        reason: &'static str,
        rearm_policy: Dci3RearmPolicy,
    ) {
        if self.has_queued_setup_input_report() {
            trace::dci3_drain_blocked(trace::Dci3DrainBlockedTrace {
                reason,
                policy: rearm_policy.name(),
                dequeue: self.slot1_dci3_dequeue,
                ring_base: self.slot1_dci3_ring_base,
                dcs: self.slot1_dci3_dcs,
                two_entry_rearm: self.slot1_dci3_two_entry_queue_rearm,
                last_dequeue: self.slot1_dci3_last_dequeue,
                last_dcs: self.slot1_dci3_last_dcs,
                last_ring_base: self.slot1_dci3_last_ring_base,
                last_ring_dcs: self.slot1_dci3_last_ring_dcs,
                queued_reports: self.setup_input_report_stats.queued_reports,
                emitted_key_reports: self.setup_input_report_stats.emitted_key_reports,
                emitted_release_reports: self.setup_input_report_stats.emitted_release_reports,
            });
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
