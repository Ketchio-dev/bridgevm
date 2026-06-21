use std::collections::VecDeque;

use bridgevm_hvf::platform_virt::{MmioOp, MmioOutcome, VirtPlatform};
use bridgevm_hvf::{fwcfg::GuestMemoryMut, machine, pcie};

#[derive(Debug, Clone)]
struct PcieEcamEvent {
    pc: u64,
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
        if cfg.bdf() != pcie::XHCI_BDF || !tracked_xhci_reg(cfg.reg) {
            return;
        }
        if self.events.len() == self.max {
            self.events.pop_front();
        }
        self.events.push_back(PcieEcamEvent {
            pc,
            reg: cfg.reg,
            op: describe_mmio_op(op),
            outcome: describe_mmio_outcome(outcome),
            readback: pcie_cfg_read(platform, mem, cfg.bdf(), cfg.reg, access_size(op)),
            command_status: pcie_cfg_read(platform, mem, cfg.bdf(), pcie::REG_COMMAND_STATUS, 4)
                .and_then(|value| u32::try_from(value).ok()),
            bars: xhci_bar_readbacks(platform, mem),
            msix_control: pcie_cfg_read(
                platform,
                mem,
                cfg.bdf(),
                u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 2,
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
            "recent xHCI PCIe ECAM config events (last {}):",
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
    let (bus, device, function) = pcie::XHCI_BDF;
    let mut line = format!(
        "  pc={:#x} reg={bus:02x}:{device:02x}.{function} {}+{:#x} op={} outcome={}",
        event.pc,
        xhci_reg_label(event.reg),
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

fn tracked_xhci_reg(reg: u16) -> bool {
    let command_status = pcie::REG_COMMAND_STATUS..pcie::REG_COMMAND_STATUS + 4;
    let xhci_bar = pcie::REG_BAR0..pcie::REG_BAR0 + 8;
    let msix = u16::from(pcie::XHCI_MSIX_CAP_OFFSET)..u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 12;
    command_status.contains(&reg) || xhci_bar.contains(&reg) || msix.contains(&reg)
}

fn xhci_reg_label(reg: u16) -> String {
    if (pcie::REG_COMMAND_STATUS..pcie::REG_COMMAND_STATUS + 4).contains(&reg) {
        return "command/status".to_string();
    }
    if (pcie::REG_BAR0..pcie::REG_BAR0 + 4).contains(&reg) {
        return "BAR0".to_string();
    }
    if (pcie::REG_BAR0 + 4..pcie::REG_BAR0 + 8).contains(&reg) {
        return "BAR1".to_string();
    }
    if (u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 2..u16::from(pcie::XHCI_MSIX_CAP_OFFSET) + 4)
        .contains(&reg)
    {
        return "msix.message_control".to_string();
    }
    "cfg".to_string()
}

fn xhci_bar_readbacks(
    platform: &mut VirtPlatform,
    mem: &mut dyn GuestMemoryMut,
) -> Option<PcieBarReadbacks> {
    let bar0 = pcie_cfg_read(platform, mem, pcie::XHCI_BDF, pcie::REG_BAR0, 4)
        .and_then(|value| u32::try_from(value).ok())?;
    let bar1 = pcie_cfg_read(platform, mem, pcie::XHCI_BDF, pcie::REG_BAR0 + 4, 4)
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

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_hvf::dtb::VirtFdtConfig;

    struct NullGuestMemory;

    impl GuestMemoryMut for NullGuestMemory {
        fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
            false
        }

        fn read_bytes(&self, _gpa: u64, _len: usize) -> Option<Vec<u8>> {
            None
        }
    }

    fn new_platform() -> VirtPlatform {
        VirtPlatform::new(VirtFdtConfig {
            cpu_count: 1,
            ram_size: 512 * 1024 * 1024,
        })
    }

    #[test]
    fn xhci_command_write_event_includes_readback_state() {
        let mut platform = new_platform();
        let mut mem = NullGuestMemory;
        let mut trace = RecentPcieEcam::new(4);
        let gpa = pcie_ecam_gpa(pcie::XHCI_BDF, pcie::REG_COMMAND_STATUS);
        let op = MmioOp::Write {
            size: 2,
            value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
        };
        let outcome = platform.on_mmio(gpa, op, &mut mem);

        trace.record_after(&mut platform, &mut mem, 0x1234, gpa, &op, &outcome);

        let lines = trace.event_lines().join("\n");
        assert!(lines.contains("00:02.0 command/status"));
        assert!(lines.contains("op=write2(0x6)"));
        assert!(lines.contains("outcome=write-ack"));
        assert!(lines.contains("readback=0x00000006"));
        assert!(lines.contains("command=0x0006"));
        assert!(lines.contains("memory=true"));
        assert!(lines.contains("bus_master=true"));
    }

    #[test]
    fn xhci_command_clear_event_reports_decode_disabled() {
        let mut platform = new_platform();
        let mut mem = NullGuestMemory;
        let mut trace = RecentPcieEcam::new(4);
        let gpa = pcie_ecam_gpa(pcie::XHCI_BDF, pcie::REG_COMMAND_STATUS);
        let enable = MmioOp::Write {
            size: 2,
            value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
        };
        let clear = MmioOp::Write { size: 2, value: 0 };
        let _ = platform.on_mmio(gpa, enable, &mut mem);
        let outcome = platform.on_mmio(gpa, clear, &mut mem);

        trace.record_after(&mut platform, &mut mem, 0x5678, gpa, &clear, &outcome);

        let lines = trace.event_lines().join("\n");
        assert!(lines.contains("op=write2(0x0)"));
        assert!(lines.contains("readback=0x00000000"));
        assert!(lines.contains("command=0x0000"));
        assert!(lines.contains("memory=false"));
        assert!(lines.contains("bus_master=false"));
    }
}
