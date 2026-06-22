use std::collections::VecDeque;

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::pcie::{self, PcieMmioTarget};
use bridgevm_hvf::platform_virt::MmioOp;

#[path = "xhci_trace/context.rs"]
mod context;
#[cfg(test)]
#[path = "xhci_trace/context_tests.rs"]
mod context_tests;
#[cfg(test)]
#[path = "xhci_trace/test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "xhci_trace/tests.rs"]
mod tests;
#[path = "xhci_trace/trb.rs"]
mod trb;

const XHCI_BAR0: usize = 0;
const CRCR: u64 = 0x58;
const CRCR_HI: u64 = 0x5c;
const DCBAAP: u64 = 0x70;
const CONFIG: u64 = 0x78;
const ERSTSZ0: u64 = 0x1028;
const ERSTBA0: u64 = 0x1030;
const ERDP0: u64 = 0x1038;
const DOORBELL_BASE: u64 = 0x2000;
const DOORBELL_STRIDE: u64 = 4;
const MAX_DOORBELL_INDEX: u64 = 64;
const COMMAND_RING_POINTER_MASK: u64 = !0x3f;
const LINK_TRB_POINTER_MASK: u64 = !0xf;
const DEFAULT_MAX_EVENTS: usize = 160;
const MAX_TRANSFER_TRBS_TO_DUMP: u64 = 4;

#[derive(Debug)]
pub(super) struct XhciBringupTrace {
    max: usize,
    events: VecDeque<String>,
    print_events: bool,
    raw_crcr: u64,
    command_dequeue: u64,
    command_cycle: bool,
    endpoint_contexts: context::EndpointContexts,
}

impl XhciBringupTrace {
    pub(super) fn new(max: usize) -> Self {
        Self {
            max,
            events: VecDeque::with_capacity(max.min(DEFAULT_MAX_EVENTS)),
            print_events: false,
            raw_crcr: 0,
            command_dequeue: 0,
            command_cycle: false,
            endpoint_contexts: context::EndpointContexts::new(),
        }
    }

    pub(super) fn print_events_immediately(&mut self, enabled: bool) {
        self.print_events = enabled;
    }

    pub(super) fn record_mmio(
        &mut self,
        target: Option<PcieMmioTarget>,
        op: &MmioOp,
        mem: &dyn GuestMemoryMut,
    ) {
        let Some(target) = target else {
            return;
        };
        if target.bdf != pcie::XHCI_BDF || target.bar_index != XHCI_BAR0 {
            return;
        }
        let MmioOp::Write { size, value } = *op else {
            return;
        };
        let value = mask_to_size(value, size);
        self.record_write(target.offset, size, value, mem);
    }

    pub(super) fn print(&self) {
        if self.events.is_empty() {
            return;
        }
        println!("recent xHCI bring-up trace (last {}):", self.events.len());
        for event in &self.events {
            println!("  {event}");
        }
    }

    fn record_write(&mut self, offset: u64, size: u8, value: u64, mem: &dyn GuestMemoryMut) {
        match (offset, size) {
            (CRCR, 8) => self.write_crcr(value),
            (CRCR, 4) => self.write_crcr((self.raw_crcr & !0xffff_ffff) | value),
            (CRCR_HI, 4) => self.write_crcr((self.raw_crcr & 0xffff_ffff) | (value << 32)),
            (DCBAAP, 8) => self.push(format!("dcbaap={value:#x}")),
            (CONFIG, 4) => self.push(format!("config={value:#x}")),
            (ERSTSZ0, 4) => self.push(format!("erstsz0={value:#x}")),
            (ERSTBA0, 8) => self.push(format!("erstba0={value:#x}")),
            (ERDP0, 8) => self.push(format!("erdp0={value:#x}")),
            _ => {}
        }

        if let Some(index) = doorbell_index(offset, size) {
            self.record_doorbell(index, value as u32, mem);
        }
    }

    fn write_crcr(&mut self, value: u64) {
        self.raw_crcr = value;
        self.command_dequeue = value & COMMAND_RING_POINTER_MASK;
        self.command_cycle = value & trb::CYCLE as u64 != 0;
        self.push(format!(
            "crcr={value:#x} command_dequeue={:#x} cycle={}",
            self.command_dequeue, self.command_cycle
        ));
    }

    fn record_doorbell(&mut self, index: u64, value: u32, mem: &dyn GuestMemoryMut) {
        if index == 0 {
            self.push(format!(
                "doorbell[0] command value={value:#x} dequeue={:#x} cycle={}",
                self.command_dequeue, self.command_cycle
            ));
            self.record_command_ring(mem);
            return;
        }

        let slot = index as usize;
        let target = value & 0xff;
        let ep0_dequeue = self.endpoint_contexts.ep0_dequeue(slot);
        let dci3_dequeue = self.endpoint_contexts.dci3_dequeue(slot);
        self.push(format!(
            "doorbell[{index}] slot={index} target={target:#x} value={value:#x} ep0_dequeue={ep0_dequeue:#x} dci3_dequeue={dci3_dequeue:#x}"
        ));
        let (ring_name, dequeue) = self.endpoint_contexts.ring_for_target(slot, target);
        if dequeue != 0 {
            self.dump_transfer_ring(slot, target, ring_name, dequeue, mem);
        }
    }

    fn record_command_ring(&mut self, mem: &dyn GuestMemoryMut) {
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
            trb::TYPE_ENABLE_SLOT | trb::TYPE_DISABLE_SLOT | trb::TYPE_EVALUATE_CONTEXT => {
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

    fn dump_transfer_ring(
        &mut self,
        slot: usize,
        target: u32,
        ring_name: &str,
        dequeue: u64,
        mem: &dyn GuestMemoryMut,
    ) {
        for trb_index in 0..MAX_TRANSFER_TRBS_TO_DUMP {
            let Some(gpa) = dequeue.checked_add(trb_index * trb::BYTES_U64) else {
                self.push(format!(
                    "transfer_trb slot={slot} target={target:#x} ring={ring_name} index={trb_index} gpa=overflow dequeue={dequeue:#x}"
                ));
                return;
            };
            let Some(transfer) = trb::Trb::read_from(mem, gpa) else {
                self.push(format!(
                    "transfer_trb slot={slot} target={target:#x} ring={ring_name} index={trb_index} gpa={gpa:#x} unreadable"
                ));
                return;
            };
            let setup = transfer.setup_description();
            self.push(format!(
                "transfer_trb slot={slot} target={target:#x} ring={ring_name} index={trb_index} gpa={gpa:#x} type={} parameter={parameter:#x} status={status:#x} control={control:#x}{setup}",
                transfer.kind_name(),
                parameter = transfer.parameter,
                status = transfer.status,
                control = transfer.control,
            ));
        }
    }

    fn advance_command_dequeue(&mut self, command_gpa: u64) {
        if let Some(next) = command_gpa.checked_add(trb::BYTES_U64) {
            self.command_dequeue = next;
        }
    }

    fn push(&mut self, event: String) {
        if self.max == 0 {
            return;
        }
        if self.events.len() == self.max {
            self.events.pop_front();
        }
        if self.print_events {
            println!("xhci bring-up: {event}");
        }
        self.events.push_back(event);
    }
}

fn doorbell_index(offset: u64, size: u8) -> Option<u64> {
    if size != 4 || offset < DOORBELL_BASE || offset % DOORBELL_STRIDE != 0 {
        return None;
    }
    let index = (offset - DOORBELL_BASE) / DOORBELL_STRIDE;
    (index <= MAX_DOORBELL_INDEX).then_some(index)
}

fn mask_to_size(value: u64, size: u8) -> u64 {
    match size {
        1 => value & 0xff,
        2 => value & 0xffff,
        4 => value & 0xffff_ffff,
        _ => value,
    }
}
