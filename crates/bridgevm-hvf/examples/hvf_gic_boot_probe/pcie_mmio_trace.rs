use std::collections::VecDeque;

use bridgevm_hvf::machine;
use bridgevm_hvf::nvme;
use bridgevm_hvf::pcie;
use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome};

#[derive(Debug, Clone)]
struct RecentMmioEvent {
    pc: u64,
    ipa: u64,
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
        for event in &self.events {
            let off = event.ipa.saturating_sub(machine::PCIE_MMIO_32.base);
            println!(
                "  pc={:#x} ipa={:#x} off={off:#x} reg={} op={} outcome={}",
                event.pc,
                event.ipa,
                pcie_mmio_register_name(off),
                event.op,
                event.outcome
            );
        }
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

fn pcie_mmio_register_name(offset: u64) -> String {
    if offset < u64::from(pcie::NVME_BAR0_SIZE) {
        return nvme_bar0_register_name(offset);
    }
    format!("pcie-mmio-32+{offset:#x}")
}

fn nvme_bar0_register_name(offset: u64) -> String {
    match offset {
        nvme::REG_CAP => "nvme.CAP".to_string(),
        o if o == nvme::REG_CAP + 4 => "nvme.CAP+4".to_string(),
        nvme::REG_VS => "nvme.VS".to_string(),
        nvme::REG_INTMS => "nvme.INTMS".to_string(),
        nvme::REG_INTMC => "nvme.INTMC".to_string(),
        nvme::REG_CC => "nvme.CC".to_string(),
        nvme::REG_CSTS => "nvme.CSTS".to_string(),
        nvme::REG_AQA => "nvme.AQA".to_string(),
        nvme::REG_ASQ => "nvme.ASQ".to_string(),
        o if o == nvme::REG_ASQ + 4 => "nvme.ASQ+4".to_string(),
        nvme::REG_ACQ => "nvme.ACQ".to_string(),
        o if o == nvme::REG_ACQ + 4 => "nvme.ACQ+4".to_string(),
        o if (nvme::REG_DOORBELL_BASE..nvme::REG_DOORBELL_END).contains(&o) && o % 4 == 0 => {
            let index = (o - nvme::REG_DOORBELL_BASE) / 4;
            let qid = index / 2;
            if index % 2 == 0 {
                format!("nvme.SQ{qid}TDBL")
            } else {
                format!("nvme.CQ{qid}HDBL")
            }
        }
        o if (u64::from(pcie::NVME_MSIX_TABLE_OFFSET)
            ..u64::from(pcie::NVME_MSIX_TABLE_OFFSET)
                + u64::from(pcie::NVME_MSIX_VECTOR_COUNT) * 16)
            .contains(&o) =>
        {
            let table_off = o - u64::from(pcie::NVME_MSIX_TABLE_OFFSET);
            let vector = table_off / 16;
            let field = match table_off % 16 {
                0..=3 => "addr_lo",
                4..=7 => "addr_hi",
                8..=11 => "data",
                _ => "vector_ctl",
            };
            format!("nvme.msix.table[{vector}].{field}")
        }
        o if (u64::from(pcie::NVME_MSIX_PBA_OFFSET)..u64::from(pcie::NVME_MSIX_PBA_OFFSET) + 8)
            .contains(&o) =>
        {
            "nvme.msix.pba".to_string()
        }
        _ => format!("nvme.bar0+{offset:#x}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn decodes_nvme_doorbell_registers() {
        assert_eq!(pcie_mmio_register_name(0x1000), "nvme.SQ0TDBL");
        assert_eq!(pcie_mmio_register_name(0x1004), "nvme.CQ0HDBL");
        assert_eq!(pcie_mmio_register_name(0x1008), "nvme.SQ1TDBL");
        assert_eq!(pcie_mmio_register_name(0x100c), "nvme.CQ1HDBL");
    }

    #[test]
    fn decodes_nvme_registers_and_msix_table() {
        assert_eq!(pcie_mmio_register_name(nvme::REG_CSTS), "nvme.CSTS");
        assert_eq!(
            pcie_mmio_register_name(u64::from(pcie::NVME_MSIX_TABLE_OFFSET) + 24),
            "nvme.msix.table[1].data"
        );
        assert_eq!(
            pcie_mmio_register_name(u64::from(pcie::NVME_MSIX_PBA_OFFSET)),
            "nvme.msix.pba"
        );
    }
}
