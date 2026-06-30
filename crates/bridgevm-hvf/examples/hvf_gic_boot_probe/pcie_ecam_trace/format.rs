use bridgevm_hvf::pcie;

/// MSI-X Message Control "Function Mask" bit (PCIe Base Spec, bit 14). Kept local to this
/// diagnostic formatter because `pcie.rs` is out of scope for this trace-enrichment change.
const MSIX_FUNCTION_MASK: u16 = 1 << 14;

/// Classify a tracked endpoint purely from its BDF (diagnostic label only).
fn endpoint_label(bdf: (u8, u8, u8)) -> &'static str {
    if bdf == pcie::NVME_BDF {
        "nvme"
    } else if bdf == pcie::XHCI_BDF {
        "xhci"
    } else {
        "other"
    }
}

pub(super) fn format_event_line(event: &super::PcieEcamEvent) -> String {
    let (bus, device, function) = event.bdf;
    let owner = &event.owner_context;
    let mut line = format!(
        "  pc={:#x} exit={} ipa={:#x} esr={:#x} ec={:#x} srt={} serial_phase={} bdf={bus:02x}:{device:02x}.{function} reg={}+{:#x} access={} op={} outcome={}",
        event.pc,
        owner.exit,
        owner.ipa,
        owner.esr,
        owner.ec,
        owner.srt,
        owner.serial_phase,
        super::tracked_reg_label(event),
        event.reg,
        event.access,
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

    line.push_str(&format!(" endpoint={}", endpoint_label(event.bdf)));
    if let Some(command_status) = event.command_status {
        let command = (command_status & u32::from(u16::MAX)) as u16;
        let decode_enabled = command & (pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER) != 0;
        let command_effect = if event.access != "write" {
            "unchanged"
        } else if decode_enabled {
            "enabled"
        } else {
            "disabled"
        };
        line.push_str(&format!(" command_effect={command_effect}"));
    }
    if let Some(bars) = event.bars {
        let bar0_assigned = bars.bar0 & !0xf != 0;
        line.push_str(&format!(" bar0_assigned={bar0_assigned}"));
    }
    if let Some(msix_control) = event.msix_control {
        let msix_masked = msix_control & MSIX_FUNCTION_MASK != 0;
        line.push_str(&format!(" msix_masked={msix_masked}"));
    }
    line
}
