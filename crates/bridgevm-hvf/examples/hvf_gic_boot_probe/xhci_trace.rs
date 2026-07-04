use std::collections::VecDeque;

use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::pcie::{self, PcieMmioTarget};
use bridgevm_hvf::platform_virt::MmioOp;
use bridgevm_hvf::xhci::XhciEventLifecycleStats;

#[cfg(test)]
#[path = "xhci_trace/command_lifecycle_tests.rs"]
mod command_lifecycle_tests;
#[path = "xhci_trace/command_ring_trace.rs"]
mod command_ring_trace;
#[cfg(test)]
#[path = "xhci_trace/command_trace_tests.rs"]
mod command_trace_tests;
#[path = "xhci_trace/context.rs"]
mod context;
#[cfg(test)]
#[path = "xhci_trace/context_configure_stale_tests.rs"]
mod context_configure_stale_tests;
#[cfg(test)]
#[path = "xhci_trace/context_configure_tests.rs"]
mod context_configure_tests;
#[cfg(test)]
#[path = "xhci_trace/context_tests.rs"]
mod context_tests;
#[path = "xhci_trace/lifecycle_summary.rs"]
mod lifecycle_summary;
#[path = "xhci_trace/registers.rs"]
mod registers;
#[cfg(test)]
#[path = "xhci_trace/test_support.rs"]
mod test_support;
#[cfg(test)]
#[path = "xhci_trace/tests.rs"]
mod tests;
#[path = "xhci_trace/trb.rs"]
mod trb;

use lifecycle_summary::InterruptEndpointLifecycleSummary;
use registers::{
    COMMAND_RING_POINTER_MASK, CONFIG, CRCR, CRCR_HI, DCBAAP, DEFAULT_MAX_EVENTS, ERDP0, ERDP0_HI,
    ERSTBA0, ERSTSZ0, XHCI_BAR0,
};
#[cfg(test)]
use registers::{DOORBELL_BASE, DOORBELL_STRIDE};

#[derive(Debug)]
pub(super) struct XhciBringupTrace {
    max: usize,
    events: VecDeque<String>,
    print_events: bool,
    raw_crcr: u64,
    raw_erdp0: u64,
    command_dequeue: u64,
    command_cycle: bool,
    endpoint_contexts: context::EndpointContexts,
    lifecycle: InterruptEndpointLifecycleSummary,
}

impl XhciBringupTrace {
    pub(super) fn new(max: usize) -> Self {
        Self {
            max,
            events: VecDeque::with_capacity(max.min(DEFAULT_MAX_EVENTS)),
            print_events: false,
            raw_crcr: 0,
            raw_erdp0: 0,
            command_dequeue: 0,
            command_cycle: false,
            endpoint_contexts: context::EndpointContexts::new(),
            lifecycle: InterruptEndpointLifecycleSummary::default(),
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
        let value = registers::mask_to_size(value, size);
        self.record_write(target.offset, size, value, mem);
    }

    pub(super) fn print(&self, event_stats: XhciEventLifecycleStats) {
        let summary = self.summary_lines(event_stats);
        if !summary.is_empty() {
            println!("xHCI interrupt endpoint lifecycle summary:");
            for line in summary {
                println!("  {line}");
            }
        }
        if self.events.is_empty() {
            return;
        }
        println!("recent xHCI bring-up trace (last {}):", self.events.len());
        for event in &self.events {
            println!("  {event}");
        }
    }

    fn summary_lines(&self, event_stats: XhciEventLifecycleStats) -> Vec<String> {
        self.lifecycle.summary_lines(event_stats)
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
            (ERDP0, 8) => self.write_erdp0(value),
            (ERDP0, 4) => self.write_erdp0((self.raw_erdp0 & !0xffff_ffff) | value),
            (ERDP0_HI, 4) => self.write_erdp0((self.raw_erdp0 & 0xffff_ffff) | (value << 32)),
            _ => {}
        }

        if let Some(index) = registers::doorbell_index(offset, size) {
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

    fn write_erdp0(&mut self, value: u64) {
        self.raw_erdp0 = value;
        self.lifecycle.record_guest_erdp0(value);
        self.push(format!("erdp0={value:#x}"));
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
        let dci5_dequeue = self.endpoint_contexts.dci5_dequeue(slot);
        self.lifecycle.record_doorbell(
            slot,
            target,
            value,
            ep0_dequeue,
            dci3_dequeue,
            dci5_dequeue,
        );
        self.push(format!(
            "doorbell[{index}] slot={index} target={target:#x} value={value:#x} ep0_dequeue={ep0_dequeue:#x} dci3_dequeue={dci3_dequeue:#x} dci5_dequeue={dci5_dequeue:#x}"
        ));
        let (ring_name, dequeue) = self.endpoint_contexts.ring_for_target(slot, target);
        if dequeue != 0 {
            self.dump_transfer_ring(slot, target, ring_name, dequeue, mem);
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
