use super::{context, trb};

#[path = "lifecycle_format.rs"]
mod lifecycle_format;

#[derive(Debug, Default)]
pub(super) struct InterruptEndpointLifecycleSummary {
    set_tr_dequeue_pointer_count: u64,
    last_set_tr_dequeue_pointer: Option<SetTrDequeuePointerSummary>,
    dci5_set_tr_dequeue_pointer_count: u64,
    last_dci5_set_tr_dequeue_pointer: Option<SetTrDequeuePointerSummary>,
    doorbell_count: u64,
    last_doorbell: Option<DoorbellSummary>,
    dci5_doorbell_count: u64,
    last_dci5_doorbell: Option<DoorbellSummary>,
    transfer_ring_snapshot_count: u64,
    last_transfer_ring_snapshot: Option<TransferRingSnapshot>,
    dci5_transfer_ring_snapshot_count: u64,
    last_dci5_transfer_ring_snapshot: Option<TransferRingSnapshot>,
    guest_erdp0_write_count: u64,
    last_guest_erdp0: Option<u64>,
}

#[derive(Debug, Clone, Copy)]
struct SetTrDequeuePointerSummary {
    slot: u32,
    endpoint: u32,
    raw_dequeue: u64,
    dequeue: u64,
    dcs: bool,
}

#[derive(Debug, Clone, Copy)]
struct DoorbellSummary {
    slot: usize,
    target: u32,
    value: u32,
    ep0_dequeue: u64,
    dci3_dequeue: u64,
    dci5_dequeue: u64,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct TransferRingSnapshot {
    slot: usize,
    target: u32,
    dequeue: u64,
    trbs_read: u64,
    nonzero_trbs: u64,
    zero_type0_trbs: u64,
    first_nonzero_index: Option<u64>,
    first_zero_type0_index: Option<u64>,
    overflow: bool,
    unreadable: bool,
}

impl InterruptEndpointLifecycleSummary {
    pub(super) fn has_observations(&self) -> bool {
        self.set_tr_dequeue_pointer_count != 0
            || self.dci5_set_tr_dequeue_pointer_count != 0
            || self.doorbell_count != 0
            || self.dci5_doorbell_count != 0
            || self.transfer_ring_snapshot_count != 0
            || self.dci5_transfer_ring_snapshot_count != 0
            || self.guest_erdp0_write_count != 0
    }

    pub(super) fn record_set_tr_dequeue_pointer(&mut self, command: trb::Trb) {
        let raw_dequeue = command.parameter;
        let last = SetTrDequeuePointerSummary {
            slot: command.slot_id(),
            endpoint: command.endpoint_id(),
            raw_dequeue,
            dequeue: raw_dequeue & context::TRANSFER_RING_POINTER_MASK,
            dcs: raw_dequeue & u64::from(trb::CYCLE) != 0,
        };
        match command.endpoint_id() {
            context::DCI3 => {
                self.set_tr_dequeue_pointer_count =
                    self.set_tr_dequeue_pointer_count.saturating_add(1);
                self.last_set_tr_dequeue_pointer = Some(last);
            }
            context::DCI5 => {
                self.dci5_set_tr_dequeue_pointer_count =
                    self.dci5_set_tr_dequeue_pointer_count.saturating_add(1);
                self.last_dci5_set_tr_dequeue_pointer = Some(last);
            }
            _ => {}
        }
    }

    pub(super) fn record_doorbell(
        &mut self,
        slot: usize,
        target: u32,
        value: u32,
        ep0_dequeue: u64,
        dci3_dequeue: u64,
        dci5_dequeue: u64,
    ) {
        let last = DoorbellSummary {
            slot,
            target,
            value,
            ep0_dequeue,
            dci3_dequeue,
            dci5_dequeue,
        };
        match target {
            context::DCI3 => {
                self.doorbell_count = self.doorbell_count.saturating_add(1);
                self.last_doorbell = Some(last);
            }
            context::DCI5 => {
                self.dci5_doorbell_count = self.dci5_doorbell_count.saturating_add(1);
                self.last_dci5_doorbell = Some(last);
            }
            _ => {}
        }
    }

    pub(super) fn record_transfer_ring_snapshot(&mut self, snapshot: TransferRingSnapshot) {
        match snapshot.target {
            context::DCI3 => {
                self.transfer_ring_snapshot_count =
                    self.transfer_ring_snapshot_count.saturating_add(1);
                self.last_transfer_ring_snapshot = Some(snapshot);
            }
            context::DCI5 => {
                self.dci5_transfer_ring_snapshot_count =
                    self.dci5_transfer_ring_snapshot_count.saturating_add(1);
                self.last_dci5_transfer_ring_snapshot = Some(snapshot);
            }
            _ => {}
        }
    }

    pub(super) fn record_guest_erdp0(&mut self, value: u64) {
        self.guest_erdp0_write_count = self.guest_erdp0_write_count.saturating_add(1);
        self.last_guest_erdp0 = Some(value);
    }
}

impl TransferRingSnapshot {
    pub(super) fn new(slot: usize, target: u32, dequeue: u64) -> Self {
        Self {
            slot,
            target,
            dequeue,
            trbs_read: 0,
            nonzero_trbs: 0,
            zero_type0_trbs: 0,
            first_nonzero_index: None,
            first_zero_type0_index: None,
            overflow: false,
            unreadable: false,
        }
    }

    pub(super) fn record_trb(&mut self, index: u64, transfer: trb::Trb) {
        self.trbs_read = self.trbs_read.saturating_add(1);
        let zero_type0 = transfer.kind() == 0
            && transfer.parameter == 0
            && transfer.status == 0
            && transfer.control == 0;
        if zero_type0 {
            self.zero_type0_trbs = self.zero_type0_trbs.saturating_add(1);
            self.first_zero_type0_index.get_or_insert(index);
            return;
        }
        self.nonzero_trbs = self.nonzero_trbs.saturating_add(1);
        self.first_nonzero_index.get_or_insert(index);
    }

    pub(super) fn mark_overflow(&mut self) {
        self.overflow = true;
    }

    pub(super) fn mark_unreadable(&mut self) {
        self.unreadable = true;
    }
}
