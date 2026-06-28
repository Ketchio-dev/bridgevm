use std::collections::VecDeque;

use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome, VirtPlatform};
use bridgevm_hvf::{fwcfg::GuestMemoryMut, machine, pcie};

#[cfg(test)]
#[path = "pcie_ecam_trace/tests.rs"]
mod tests;

#[derive(Debug, Clone)]
struct PcieEcamEvent {
    pc: u64,
    bdf: (u8, u8, u8),
    reg: u16,
    op: String,
    outcome: String,
    readback: Option<u64>,
    command_status: Option<u32>,
    bars: Option<PcieBarReadbacks>,
    msix_control: Option<u16>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PcieBarReadbacks {
    bar0: u32,
    bar1: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TrackedEndpoint {
    bdf: (u8, u8, u8),
    msix_cap_offset: u8,
}

const TRACKED_ENDPOINTS: [TrackedEndpoint; 2] = [
    TrackedEndpoint {
        bdf: pcie::NVME_BDF,
        msix_cap_offset: pcie::NVME_MSIX_CAP_OFFSET,
    },
    TrackedEndpoint {
        bdf: pcie::XHCI_BDF,
        msix_cap_offset: pcie::XHCI_MSIX_CAP_OFFSET,
    },
];

#[derive(Debug)]
pub(super) struct RecentPcieEcam {
    max: usize,
    events: VecDeque<PcieEcamEvent>,
}

impl RecentPcieEcam {
    pub(super) fn new(max: usize) -> Self {
        Self {
            max,
            events: VecDeque::with_capacity(max.min(1024)),
        }
    }

    pub(super) fn record_after(
        &mut self,
        platform: &mut VirtPlatform,
        mem: &mut dyn GuestMemoryMut,
        pc: u64,
        ipa: u64,
        op: &MmioOp,
        outcome: &MmioOutcome,
    ) {
        if self.max == 0 || !machine::PCIE_ECAM.contains(ipa) {
            return;
        }
        let cfg = pcie::CfgAddr::from_ecam_offset(ipa - machine::PCIE_ECAM.base);
        let Some(endpoint) = tracked_endpoint(cfg.bdf()) else {
            return;
        };
        if !tracked_reg(endpoint, cfg.reg) {
            return;
        }
        if self.events.len() == self.max {
            self.events.pop_front();
        }
        self.events.push_back(PcieEcamEvent {
            pc,
            bdf: cfg.bdf(),
            reg: cfg.reg,
            op: describe_mmio_op(op),
            outcome: describe_mmio_outcome(outcome),
            readback: pcie_cfg_read(platform, mem, cfg.bdf(), cfg.reg, access_size(op)),
            command_status: pcie_cfg_read(platform, mem, cfg.bdf(), pcie::REG_COMMAND_STATUS, 4)
                .and_then(|value| u32::try_from(value).ok()),
            bars: pcie_bar_readbacks(platform, mem, cfg.bdf()),
            msix_control: pcie_cfg_read(
                platform,
                mem,
                cfg.bdf(),
                u16::from(endpoint.msix_cap_offset) + 2,
                2,
            )
            .and_then(|value| u16::try_from(value).ok()),
        });
    }

    pub(super) fn print(&self) {
        if self.events.is_empty() {
            return;
        }
        println!(
            "recent PCIe ECAM lifecycle events (last {}):",
            self.events.len()
        );
        for line in self.event_lines() {
            println!("{line}");
        }
    }

    fn event_lines(&self) -> Vec<String> {
        self.events.iter().map(format_event_line).collect()
    }
}

fn format_event_line(event: &PcieEcamEvent) -> String {
    let (bus, device, function) = event.bdf;
    let mut line = format!(
        "  pc={:#x} bdf={bus:02x}:{device:02x}.{function} reg={}+{:#x} op={} outcome={}",
        event.pc,
        tracked_reg_label(event),
        event.reg,
        event.op,
        event.outcome
    );
    if let Some(readback) = event.readback {
        line.push_str(&format!(" readback={readback:#010x}"));
    }
    if let Some(command_status) = event.command_status {
        let command = (command_status & u32::from(u16::MAX)) as u16;
        let status = command_status >> 16;
        let memory = command & pcie::CMD_MEMORY_SPACE != 0;
        let bus_master = command & pcie::CMD_BUS_MASTER != 0;
        line.push_str(&format!(
            " command={command:#06x} memory={memory} bus_master={bus_master} status={status:#06x}"
        ));
    }
    if let Some(bars) = event.bars {
        let base = (u64::from(bars.bar1) << 32) | u64::from(bars.bar0 & !0xf);
        line.push_str(&format!(
            " bar0={:#010x} bar1={:#010x} base={base:#x}",
            bars.bar0, bars.bar1
        ));
    }
    if let Some(msix_control) = event.msix_control {
        line.push_str(&format!(" msix_ctrl={msix_control:#06x}"));
    }
    line
}

fn tracked_endpoint(bdf: (u8, u8, u8)) -> Option<TrackedEndpoint> {
    TRACKED_ENDPOINTS
        .iter()
        .copied()
        .find(|endpoint| endpoint.bdf == bdf)
}

fn tracked_reg(endpoint: TrackedEndpoint, reg: u16) -> bool {
    let command_status = pcie::REG_COMMAND_STATUS..pcie::REG_COMMAND_STATUS + 4;
    let bar = pcie::REG_BAR0..pcie::REG_BAR0 + 8;
    let msix = u16::from(endpoint.msix_cap_offset)..u16::from(endpoint.msix_cap_offset) + 12;
    command_status.contains(&reg) || bar.contains(&reg) || msix.contains(&reg)
}

fn tracked_reg_label(event: &PcieEcamEvent) -> String {
    let msix_cap_offset = tracked_endpoint(event.bdf)
        .map(|endpoint| endpoint.msix_cap_offset)
        .unwrap_or(0);
    let reg = event.reg;
    if (pcie::REG_COMMAND_STATUS..pcie::REG_COMMAND_STATUS + 4).contains(&reg) {
        return "command/status".to_string();
    }
    if (pcie::REG_BAR0..pcie::REG_BAR0 + 4).contains(&reg) {
        return "BAR0".to_string();
    }
    if (pcie::REG_BAR0 + 4..pcie::REG_BAR0 + 8).contains(&reg) {
        return "BAR1".to_string();
    }
    if (u16::from(msix_cap_offset) + 2..u16::from(msix_cap_offset) + 4).contains(&reg) {
        return "msix.message_control".to_string();
    }
    if (u16::from(msix_cap_offset)..u16::from(msix_cap_offset) + 12).contains(&reg) {
        return "msix".to_string();
    }
    "cfg".to_string()
}

fn pcie_bar_readbacks(
    platform: &mut VirtPlatform,
    mem: &mut dyn GuestMemoryMut,
    bdf: (u8, u8, u8),
) -> Option<PcieBarReadbacks> {
    let bar0 = pcie_cfg_read(platform, mem, bdf, pcie::REG_BAR0, 4)
        .and_then(|value| u32::try_from(value).ok())?;
    let bar1 = pcie_cfg_read(platform, mem, bdf, pcie::REG_BAR0 + 4, 4)
        .and_then(|value| u32::try_from(value).ok())?;
    Some(PcieBarReadbacks { bar0, bar1 })
}

fn pcie_cfg_read(
    platform: &mut VirtPlatform,
    mem: &mut dyn GuestMemoryMut,
    bdf: (u8, u8, u8),
    reg: u16,
    size: u8,
) -> Option<u64> {
    match platform.on_mmio(pcie_ecam_gpa(bdf, reg), MmioOp::Read { size }, mem) {
        MmioOutcome::ReadValue(value) => Some(value),
        MmioOutcome::WriteAck | MmioOutcome::KnownUnimplemented(_) | MmioOutcome::Unmapped => None,
    }
}

fn pcie_ecam_gpa(bdf: (u8, u8, u8), reg: u16) -> u64 {
    let (bus, device, function) = bdf;
    machine::PCIE_ECAM.base
        + u64::from(bus)
            * u64::from(pcie::DEVICES_PER_BUS)
            * u64::from(pcie::FUNCS_PER_DEVICE)
            * pcie::CFG_SPACE_SIZE
        + u64::from(device) * u64::from(pcie::FUNCS_PER_DEVICE) * pcie::CFG_SPACE_SIZE
        + u64::from(function) * pcie::CFG_SPACE_SIZE
        + u64::from(reg)
}

fn access_size(op: &MmioOp) -> u8 {
    match op {
        MmioOp::Read { size } | MmioOp::Write { size, .. } => *size,
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
