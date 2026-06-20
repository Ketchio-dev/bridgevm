use std::collections::BTreeMap;

use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

#[derive(Debug, Clone)]
pub(super) struct MmioTrace {
    count: u64,
    reads: u64,
    writes: u64,
    last_pc: u64,
    last_ipa: u64,
    last_op: &'static str,
    last_value: Option<u64>,
    last_outcome: &'static str,
}

impl Default for MmioTrace {
    fn default() -> Self {
        Self {
            count: 0,
            reads: 0,
            writes: 0,
            last_pc: 0,
            last_ipa: 0,
            last_op: "",
            last_value: None,
            last_outcome: "",
        }
    }
}

pub(super) fn record_mmio_trace(
    traces: &mut BTreeMap<&'static str, MmioTrace>,
    device: &'static str,
    pc: u64,
    ipa: u64,
    op: MmioOp,
    outcome: &MmioOutcome,
) {
    let entry = traces.entry(device).or_default();
    entry.count += 1;
    entry.last_pc = pc;
    entry.last_ipa = ipa;
    entry.last_outcome = mmio_outcome_label(outcome);
    match op {
        MmioOp::Read { .. } => {
            entry.reads += 1;
            entry.last_op = "read";
            entry.last_value = match outcome {
                MmioOutcome::ReadValue(value) => Some(*value),
                MmioOutcome::WriteAck
                | MmioOutcome::KnownUnimplemented(_)
                | MmioOutcome::Unmapped => None,
            };
        }
        MmioOp::Write { value, .. } => {
            entry.writes += 1;
            entry.last_op = "write";
            entry.last_value = Some(value);
        }
    }
}

pub(super) fn print_mmio_traces(traces: &BTreeMap<&'static str, MmioTrace>) {
    println!("modelled MMIO trace:");
    for (device, trace) in traces {
        let value = trace
            .last_value
            .map(|v| format!("{v:#x}"))
            .unwrap_or_else(|| "-".to_string());
        println!(
            "  {device}: count={} reads={} writes={} last_pc={:#x} last_ipa={:#x} last_op={} last_value={} last_outcome={}",
            trace.count,
            trace.reads,
            trace.writes,
            trace.last_pc,
            trace.last_ipa,
            trace.last_op,
            value,
            trace.last_outcome
        );
    }
}

fn mmio_outcome_label(outcome: &MmioOutcome) -> &'static str {
    match outcome {
        MmioOutcome::ReadValue(_) => "read-value",
        MmioOutcome::WriteAck => "write-ack",
        MmioOutcome::KnownUnimplemented(_) => "known-unimplemented",
        MmioOutcome::Unmapped => "unmapped",
    }
}
