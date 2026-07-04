use bridgevm_hvf::fwcfg::GuestMemoryMut;

use super::{
    lifecycle_summary::TransferRingSnapshot,
    registers::{LINK_TRB_POINTER_MASK, MAX_TRANSFER_TRBS_TO_DUMP},
    trb, XhciBringupTrace,
};

impl XhciBringupTrace {
    pub(super) fn record_command_ring(&mut self, mem: &dyn GuestMemoryMut) {
        let command_gpa = self.command_dequeue;
        let Some(command) = trb::Trb::read_from(mem, command_gpa) else {
            self.push(format!("command_trb gpa={command_gpa:#x} unreadable"));
            return;
        };
        self.push(format!(
            "command_trb gpa={command_gpa:#x} type={} parameter={parameter:#x} status={status:#x} control={control:#x}",
            command.kind_name(),
            parameter = command.parameter,
            status = command.status,
            control = command.control,
        ));

        let expected_cycle = if self.command_cycle { trb::CYCLE } else { 0 };
        if command.cycle_bit() != expected_cycle {
            self.push(format!(
                "command_trb cycle_mismatch expected={expected_cycle:#x} control_cycle={:#x}",
                command.cycle_bit()
            ));
            return;
        }

        match command.kind() {
            trb::TYPE_LINK => {
                self.command_dequeue = command.parameter & LINK_TRB_POINTER_MASK;
            }
            trb::TYPE_ENABLE_SLOT
            | trb::TYPE_DISABLE_SLOT
            | trb::TYPE_EVALUATE_CONTEXT
            | trb::TYPE_STOP_ENDPOINT => self.advance_command_dequeue(command_gpa),
            trb::TYPE_SET_TR_DEQUEUE_POINTER => {
                self.lifecycle.record_set_tr_dequeue_pointer(command);
                if let Some(event) = self.endpoint_contexts.set_tr_dequeue_pointer(
                    command.slot_id(),
                    command.endpoint_id(),
                    command.parameter,
                ) {
                    self.push(event);
                }
                self.advance_command_dequeue(command_gpa);
            }
            trb::TYPE_ADDRESS_DEVICE => {
                if let Some(event) = self.endpoint_contexts.capture_address_device(
                    command.slot_id(),
                    command.parameter,
                    mem,
                ) {
                    self.push(event);
                }
                self.advance_command_dequeue(command_gpa);
            }
            trb::TYPE_CONFIGURE_ENDPOINT => {
                if let Some(event) = self.endpoint_contexts.capture_configure_endpoint(
                    command.slot_id(),
                    command.parameter,
                    mem,
                ) {
                    self.push(event);
                }
                self.advance_command_dequeue(command_gpa);
            }
            _ => {}
        }
    }

    pub(super) fn dump_transfer_ring(
        &mut self,
        slot: usize,
        target: u32,
        ring_name: &str,
        dequeue: u64,
        mem: &dyn GuestMemoryMut,
    ) {
        let mut snapshot = matches!(ring_name, "dci3" | "dci5")
            .then(|| TransferRingSnapshot::new(slot, target, dequeue));
        for trb_index in 0..MAX_TRANSFER_TRBS_TO_DUMP {
            let Some(gpa) = dequeue.checked_add(trb_index * trb::BYTES_U64) else {
                self.push(format!(
                    "transfer_trb slot={slot} target={target:#x} ring={ring_name} index={trb_index} gpa=overflow dequeue={dequeue:#x}"
                ));
                if let Some(mut snapshot) = snapshot {
                    snapshot.mark_overflow();
                    self.lifecycle.record_transfer_ring_snapshot(snapshot);
                }
                return;
            };
            let Some(transfer) = trb::Trb::read_from(mem, gpa) else {
                self.push(format!(
                    "transfer_trb slot={slot} target={target:#x} ring={ring_name} index={trb_index} gpa={gpa:#x} unreadable"
                ));
                if let Some(mut snapshot) = snapshot {
                    snapshot.mark_unreadable();
                    self.lifecycle.record_transfer_ring_snapshot(snapshot);
                }
                return;
            };
            if let Some(snapshot) = snapshot.as_mut() {
                snapshot.record_trb(trb_index, transfer);
            }
            let setup = transfer.setup_description();
            self.push(format!(
                "transfer_trb slot={slot} target={target:#x} ring={ring_name} index={trb_index} gpa={gpa:#x} type={} parameter={parameter:#x} status={status:#x} control={control:#x}{setup}",
                transfer.kind_name(),
                parameter = transfer.parameter,
                status = transfer.status,
                control = transfer.control,
            ));
        }
        if let Some(snapshot) = snapshot {
            self.lifecycle.record_transfer_ring_snapshot(snapshot);
        }
    }

    fn advance_command_dequeue(&mut self, command_gpa: u64) {
        if let Some(next) = command_gpa.checked_add(trb::BYTES_U64) {
            self.command_dequeue = next;
        }
    }
}
