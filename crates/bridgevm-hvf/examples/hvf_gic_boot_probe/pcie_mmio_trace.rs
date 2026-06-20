use std::collections::{BTreeMap, VecDeque};

use bridgevm_hvf::machine;
use bridgevm_hvf::pcie::PcieMmioTarget;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

#[path = "pcie_mmio_trace/registers.rs"]
mod registers;

use registers::pcie_mmio_register_name;

#[derive(Debug, Clone)]
struct RecentMmioEvent {
    pc: u64,
    ipa: u64,
    reg: String,
    op_kind: &'static str,
    op: String,
    outcome: String,
}

#[derive(Debug)]
pub(super) struct RecentMmio {
    device: &'static str,
    max: usize,
    events: VecDeque<RecentMmioEvent>,
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
        target: Option<PcieMmioTarget>,
        op: &MmioOp,
        outcome: &MmioOutcome,
    ) {
        if self.max == 0 || device != self.device {
            return;
        }
        if self.events.len() == self.max {
            self.events.pop_front();
        }
        self.events.push_back(RecentMmioEvent {
            pc,
            ipa,
            reg: pcie_mmio_register_name(target, ipa.saturating_sub(machine::PCIE_MMIO_32.base)),
            op_kind: mmio_op_kind(op),
            op: describe_mmio_op(op),
            outcome: describe_mmio_outcome(outcome),
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
        for event in &self.events {
            let off = event.ipa.saturating_sub(machine::PCIE_MMIO_32.base);
            println!(
                "  pc={:#x} ipa={:#x} off={off:#x} reg={} op={} outcome={}",
                event.pc, event.ipa, event.reg, event.op, event.outcome
            );
        }
    }

    fn print_register_summary(&self) {
        let mut counts: BTreeMap<(String, &'static str), usize> = BTreeMap::new();
        for event in &self.events {
            *counts
                .entry((event.reg.clone(), event.op_kind))
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

fn describe_mmio_outcome(outcome: &MmioOutcome) -> String {
    match outcome {
        MmioOutcome::ReadValue(value) => format!("read-value({value:#x})"),
        MmioOutcome::WriteAck => "write-ack".to_string(),
        MmioOutcome::KnownUnimplemented(name) => format!("known-unimplemented({name})"),
        MmioOutcome::Unmapped => "unmapped".to_string(),
    }
}
