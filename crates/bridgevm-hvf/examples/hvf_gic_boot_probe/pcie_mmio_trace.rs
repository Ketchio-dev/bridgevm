use std::collections::{BTreeMap, VecDeque};

use bridgevm_hvf::machine;
use bridgevm_hvf::pcie::{PcieMmioTarget, PciePioTarget};
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

#[path = "pcie_mmio_trace/context.rs"]
mod context;
#[path = "pcie_mmio_trace/registers.rs"]
mod registers;

pub(super) use context::targetless_xhci_trace_context;
use context::PcieTraceContext;
use registers::{pcie_mmio_register_name, pcie_pio_register_name};

#[derive(Debug, Clone, Copy)]
pub(super) enum PcieTraceTarget {
    Mmio(PcieMmioTarget),
    Pio(PciePioTarget),
}

#[derive(Debug, Clone)]
struct RecentMmioEvent {
    pc: u64,
    ipa: u64,
    target: Option<PcieTraceTarget>,
    op: MmioOp,
    outcome: RecentMmioOutcome,
    context: Option<PcieTraceContext>,
}

#[derive(Debug, Clone, Copy)]
enum RecentMmioOutcome {
    ReadValue(u64),
    WriteAck,
    KnownUnimplemented(&'static str),
    Unmapped,
}

impl From<&MmioOutcome> for RecentMmioOutcome {
    fn from(outcome: &MmioOutcome) -> Self {
        match outcome {
            MmioOutcome::ReadValue(value) => Self::ReadValue(*value),
            MmioOutcome::WriteAck => Self::WriteAck,
            MmioOutcome::KnownUnimplemented(name) => Self::KnownUnimplemented(name),
            MmioOutcome::Unmapped => Self::Unmapped,
        }
    }
}

#[derive(Debug)]
pub(super) struct RecentMmio {
    device: &'static str,
    max: usize,
    events: VecDeque<RecentMmioEvent>,
}

pub(super) struct PcieMmioEventInput<'a> {
    pub(super) device: &'static str,
    pub(super) pc: u64,
    pub(super) ipa: u64,
    pub(super) target: Option<PcieTraceTarget>,
    pub(super) op: &'a MmioOp,
    pub(super) outcome: &'a MmioOutcome,
    pub(super) context: Option<PcieTraceContext>,
}

impl RecentMmio {
    pub(super) fn new(device: &'static str, max: usize) -> Self {
        Self {
            device,
            max,
            events: VecDeque::with_capacity(max.min(1024)),
        }
    }

    pub(super) fn record(
        &mut self,
        device: &'static str,
        pc: u64,
        ipa: u64,
        target: Option<PcieTraceTarget>,
        op: &MmioOp,
        outcome: &MmioOutcome,
    ) {
        self.record_with_context(PcieMmioEventInput {
            device,
            pc,
            ipa,
            target,
            op,
            outcome,
            context: None,
        });
    }

    pub(super) fn record_with_context(&mut self, input: PcieMmioEventInput<'_>) {
        let PcieMmioEventInput {
            device,
            pc,
            ipa,
            target,
            op,
            outcome,
            context,
        } = input;
        if self.max == 0 || device != self.device {
            return;
        }
        if self.events.len() == self.max {
            self.events.pop_front();
        }
        self.events.push_back(RecentMmioEvent {
            pc,
            ipa,
            target,
            op: *op,
            outcome: outcome.into(),
            context,
        });
    }

    pub(super) fn print(&self) {
        if self.events.is_empty() {
            return;
        }
        println!(
            "recent {} MMIO events (last {}):",
            self.device,
            self.events.len()
        );
        self.print_register_summary();
        for line in self.event_lines() {
            println!("{line}");
        }
    }

    fn event_lines(&self) -> Vec<String> {
        self.events
            .iter()
            .map(|event| format_event_line(self.device, event))
            .collect()
    }

    fn print_register_summary(&self) {
        let mut counts: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
        for event in &self.events {
            *counts
                .entry((
                    register_name(self.device, event.ipa, event.target),
                    mmio_op_kind(&event.op),
                ))
                .or_insert(0) += 1;
        }
        let mut ranked: Vec<_> = counts.into_iter().collect();
        ranked.sort_by(
            |((left_reg, left_op), left_count), ((right_reg, right_op), right_count)| {
                right_count
                    .cmp(left_count)
                    .then_with(|| left_reg.cmp(right_reg))
                    .then_with(|| left_op.cmp(right_op))
            },
        );
        if ranked.is_empty() {
            return;
        }
        println!("recent {} register summary:", self.device);
        for ((reg, op_kind), count) in ranked.into_iter().take(10) {
            println!("  x{count} {op_kind} {reg}");
        }
    }
}

fn format_event_line(device: &'static str, event: &RecentMmioEvent) -> String {
    let off = event.ipa.saturating_sub(aperture_base(device));
    let reg = register_name(device, event.ipa, event.target);
    let op = describe_mmio_op(&event.op);
    let outcome = describe_mmio_outcome(event.outcome);
    let base = format!(
        "  pc={:#x} ipa={:#x} off={off:#x} reg={} op={} outcome={}",
        event.pc, event.ipa, reg, op, outcome
    );
    match event.context {
        Some(context) => format!("{base} {}", context.describe_for_ipa(event.ipa)),
        None => base,
    }
}

fn register_name(device: &'static str, ipa: u64, target: Option<PcieTraceTarget>) -> String {
    match (device, target) {
        ("pcie-mmio-32", Some(PcieTraceTarget::Mmio(target))) => {
            pcie_mmio_register_name(Some(target), ipa.saturating_sub(machine::PCIE_MMIO_32.base))
        }
        ("pcie-mmio-32", _) => {
            pcie_mmio_register_name(None, ipa.saturating_sub(machine::PCIE_MMIO_32.base))
        }
        ("pcie-pio", Some(PcieTraceTarget::Pio(target))) => {
            pcie_pio_register_name(Some(target), ipa.saturating_sub(machine::PCIE_PIO.base))
        }
        ("pcie-pio", _) => pcie_pio_register_name(None, ipa.saturating_sub(machine::PCIE_PIO.base)),
        _ => format!("{device}+{:#x}", ipa.saturating_sub(aperture_base(device))),
    }
}

fn aperture_base(device: &'static str) -> u64 {
    match device {
        "pcie-mmio-32" => machine::PCIE_MMIO_32.base,
        "pcie-pio" => machine::PCIE_PIO.base,
        _ => 0,
    }
}

fn mmio_op_kind(op: &MmioOp) -> &'static str {
    match op {
        MmioOp::Read { .. } => "read",
        MmioOp::Write { .. } => "write",
    }
}

fn describe_mmio_op(op: &MmioOp) -> String {
    match op {
        MmioOp::Read { size } => format!("read{size}"),
        MmioOp::Write { size, value } => format!("write{size}({value:#x})"),
    }
}

fn describe_mmio_outcome(outcome: RecentMmioOutcome) -> String {
    match outcome {
        RecentMmioOutcome::ReadValue(value) => format!("read-value({value:#x})"),
        RecentMmioOutcome::WriteAck => "write-ack".to_string(),
        RecentMmioOutcome::KnownUnimplemented(name) => format!("known-unimplemented({name})"),
        RecentMmioOutcome::Unmapped => "unmapped".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_hvf::pcie::{CMD_BUS_MASTER, XHCI_BDF};

    #[test]
    fn targetless_xhci_range_event_includes_decode_snapshot() {
        // Given: a recent PCIe MMIO trace event that no longer resolves to a BAR
        // target even though its GPA is inside the previously programmed xHCI BAR0.
        let mut recent = RecentMmio::new("pcie-mmio-32", 4);
        let snapshot = context::PcieConfigSnapshot::xhci(
            u32::from(CMD_BUS_MASTER),
            context::PcieBarReadbacks {
                bar0: 0x3efe_8004,
                bar1: 0,
            },
            None,
        );

        // When: the probe records a targetless xHCI-range KnownUnimplemented event.
        recent.record_with_context(PcieMmioEventInput {
            device: "pcie-mmio-32",
            pc: 0xffff_f803_8e35_9e78,
            ipa: 0x3efe_9040,
            target: None,
            op: &MmioOp::Read { size: 4 },
            outcome: &MmioOutcome::KnownUnimplemented("pcie-mmio-32"),
            context: Some(PcieTraceContext::PciConfig(snapshot)),
        });

        // Then: the printed event line carries current xHCI command/BAR state,
        // making a later live trace distinguish decode loss from missing register semantics.
        let line = recent.event_lines().join("\n");
        assert!(line.contains("pcie-mmio-32+0x2efe9040"));
        assert!(line.contains("xhci=00:02.0"));
        assert!(line.contains("command=0x0004"));
        assert!(line.contains("memory=false"));
        assert!(line.contains("bus_master=true"));
        assert!(line.contains("bar0=0x3efe8004"));
        assert!(line.contains("bar1=0x00000000"));
        assert!(line.contains("base=0x3efe8000"));
        assert!(line.contains("contains_ipa=true"));
        assert_eq!(XHCI_BDF, (0, 2, 0));
    }
}
