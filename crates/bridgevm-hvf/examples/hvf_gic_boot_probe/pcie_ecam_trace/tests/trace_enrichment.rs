use super::*;

#[test]
fn nvme_command_disable_event_includes_owner_context_fields() {
    let mut platform = new_platform();
    let mut mem = NullGuestMemory;
    let mut trace = RecentPcieEcam::new(4);
    let gpa = pcie_ecam_gpa(pcie::NVME_BDF, pcie::REG_COMMAND_STATUS);
    let enable = MmioOp::Write {
        size: 2,
        value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
    };
    let clear = MmioOp::Write { size: 2, value: 0 };
    let _ = platform.on_mmio(gpa, enable, &mut mem);
    let outcome = platform.on_mmio(gpa, clear, &mut mem);

    trace.record_after_with_context(
        &mut platform,
        &mut mem,
        PcieEcamAccess {
            pc: 0x9876,
            ipa: gpa,
            exit: 42,
            esr: 0x9341_0045,
            ec: 0x24,
            srt: 1,
            op: &clear,
            outcome: &outcome,
            owner_context: Some(PcieEcamOwnerContext {
                exit: 42,
                ipa: 0xfeed_beef,
                esr: 0x9341_0045,
                ec: 0x24,
                srt: 1,
                serial_phase: "Boot0001\\r\\n".to_string(),
            }),
        },
    );

    let lines = trace.event_lines().join("\n");
    assert!(lines.contains("bdf=00:01.0 reg=command/status"));
    assert!(lines.contains("op=write2(0x0)"));
    assert!(lines.contains("readback=0x00000000"));
    assert!(lines.contains("command=0x0000"));
    assert!(lines.contains("exit=42"));
    assert!(lines.contains("ipa=0xfeedbeef"));
    assert!(lines.contains("esr=0x93410045"));
    assert!(lines.contains("ec=0x24"));
    assert!(lines.contains("srt=1"));
    assert!(lines.contains("serial_phase=Boot0001\\r\\n"));
}

#[test]
fn owner_context_serial_phase_is_bounded_and_line_safe() {
    let mut uart = vec![b'a'; 200];
    uart.extend_from_slice(b" Boot0001\r\n");

    let serial_phase = PcieEcamOwnerContext::serial_phase_from_uart(&uart);

    assert!(!serial_phase.contains(' '));
    assert!(serial_phase.ends_with("_Boot0001\\r\\n"));
    assert!(serial_phase.len() <= 170);
}

#[test]
fn semantic_classifier_tokens_are_emitted() {
    let mut platform = new_platform();
    let mut mem = NullGuestMemory;
    let mut trace = RecentPcieEcam::new(8);

    let xhci_cmd = pcie_ecam_gpa(pcie::XHCI_BDF, pcie::REG_COMMAND_STATUS);
    let xhci_enable = MmioOp::Write {
        size: 2,
        value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
    };
    let xhci_outcome = platform.on_mmio(xhci_cmd, xhci_enable, &mut mem);
    trace.record_after(
        &mut platform,
        &mut mem,
        0x1,
        xhci_cmd,
        &xhci_enable,
        &xhci_outcome,
    );

    let nvme_cmd = pcie_ecam_gpa(pcie::NVME_BDF, pcie::REG_COMMAND_STATUS);
    let nvme_enable = MmioOp::Write {
        size: 2,
        value: u64::from(pcie::CMD_MEMORY_SPACE | pcie::CMD_BUS_MASTER),
    };
    let _ = platform.on_mmio(nvme_cmd, nvme_enable, &mut mem);
    let nvme_clear = MmioOp::Write { size: 2, value: 0 };
    let nvme_clear_outcome = platform.on_mmio(nvme_cmd, nvme_clear, &mut mem);
    trace.record_after(
        &mut platform,
        &mut mem,
        0x2,
        nvme_cmd,
        &nvme_clear,
        &nvme_clear_outcome,
    );

    let nvme_probe = MmioOp::Read { size: 4 };
    let nvme_probe_outcome = platform.on_mmio(nvme_cmd, nvme_probe, &mut mem);
    trace.record_after(
        &mut platform,
        &mut mem,
        0x3,
        nvme_cmd,
        &nvme_probe,
        &nvme_probe_outcome,
    );

    let nvme_msix_message_control =
        pcie_ecam_gpa(pcie::NVME_BDF, u16::from(pcie::NVME_MSIX_CAP_OFFSET) + 2);
    let nvme_mask_msix = MmioOp::Write {
        size: 2,
        value: 0x4000,
    };
    let nvme_mask_outcome = platform.on_mmio(nvme_msix_message_control, nvme_mask_msix, &mut mem);
    trace.record_after(
        &mut platform,
        &mut mem,
        0x4,
        nvme_msix_message_control,
        &nvme_mask_msix,
        &nvme_mask_outcome,
    );

    let lines = trace.event_lines();
    let xhci_line = lines
        .iter()
        .find(|line| line.contains("bdf=00:02.0"))
        .unwrap();
    let nvme_disable = lines
        .iter()
        .find(|line| line.contains("bdf=00:01.0") && line.contains("op=write2(0x0)"))
        .unwrap();
    let nvme_read = lines
        .iter()
        .find(|line| line.contains("bdf=00:01.0") && line.contains("access=read"))
        .unwrap();
    let nvme_masked = lines
        .iter()
        .find(|line| line.contains("bdf=00:01.0") && line.contains("reg=msix.message_control"))
        .unwrap();

    assert!(
        xhci_line.contains("endpoint=xhci"),
        "xhci endpoint token: {xhci_line}"
    );
    assert!(
        nvme_disable.contains("endpoint=nvme"),
        "nvme endpoint token: {nvme_disable}"
    );
    assert!(
        xhci_line.contains("command_effect=enabled"),
        "xhci effect: {xhci_line}"
    );
    assert!(
        nvme_disable.contains("command_effect=disabled"),
        "nvme effect: {nvme_disable}"
    );
    assert!(
        nvme_read.contains("command_effect=unchanged"),
        "nvme read effect: {nvme_read}"
    );
    assert!(
        nvme_disable.contains("bar0_assigned=false"),
        "nvme bar0 assigned: {nvme_disable}"
    );
    assert!(
        nvme_disable.contains("msix_masked=false"),
        "nvme default msix mask: {nvme_disable}"
    );
    assert!(
        nvme_masked.contains("msix_masked=true"),
        "nvme masked msix token: {nvme_masked}"
    );
}
