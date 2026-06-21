use bridgevm_hvf::dtb::VirtFdtConfig;

use super::*;

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
    assert!(lines.contains("bdf=00:02.0 reg=command/status"));
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

#[test]
fn ignores_unrelated_ecam_registers_and_devices() {
    let mut platform = new_platform();
    let mut mem = NullGuestMemory;
    let mut trace = RecentPcieEcam::new(4);
    let xhci_vendor = pcie_ecam_gpa(pcie::XHCI_BDF, pcie::REG_VENDOR_DEVICE);
    let nvme_command = pcie_ecam_gpa(pcie::NVME_BDF, pcie::REG_COMMAND_STATUS);

    for gpa in [xhci_vendor, nvme_command] {
        let op = MmioOp::Read { size: 4 };
        let outcome = platform.on_mmio(gpa, op, &mut mem);
        trace.record_after(&mut platform, &mut mem, 0x9abc, gpa, &op, &outcome);
    }

    assert!(trace.event_lines().is_empty());
}
