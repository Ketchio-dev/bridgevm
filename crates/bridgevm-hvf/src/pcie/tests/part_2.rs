//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::machine;

#[test]
fn qemu_xhci_exposes_msix_and_pcie_capabilities() {
    let ecam = PcieEcam::new();
    let status = ecam.cfg_read(ecam_offset(0, 2, 0, REG_COMMAND_STATUS), 4) >> 16;
    assert_ne!(status & u64::from(STATUS_CAP_LIST), 0);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, REG_CAP_PTR), 1),
        u64::from(XHCI_MSIX_CAP_OFFSET)
    );

    let msix = u16::from(XHCI_MSIX_CAP_OFFSET);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, msix), 1),
        u64::from(CAP_ID_MSIX)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, msix + 1), 1),
        u64::from(XHCI_PCIE_CAP_OFFSET)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, msix + 2), 2),
        u64::from(XHCI_MSIX_VECTOR_COUNT - 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, msix + 4), 4),
        u64::from(XHCI_MSIX_TABLE_OFFSET)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, msix + 8), 4),
        u64::from(XHCI_MSIX_PBA_OFFSET)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 2, 0, u16::from(XHCI_PCIE_CAP_OFFSET)), 1),
        0x10
    );
}

#[test]
fn qemu_xhci_bar0_is_64bit_16k_memory_bar() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
    let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);

    ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
    ecam.cfg_write(bar1, 4, 0xFFFF_FFFF);

    assert_eq!(ecam.cfg_read(bar0, 4), 0xffff_c004);
    assert_eq!(ecam.cfg_read(bar1, 4), 0xffff_ffff);
}

#[test]
fn qemu_xhci_64bit_bar_decodes_low_mmio_after_command_enable() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
    let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
    let cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
    let base = machine::PCIE_MMIO_32.base + 0x2_0000;

    ecam.cfg_write(bar0, 4, base);
    ecam.cfg_write(bar1, 4, 0);
    assert_eq!(ecam.mmio_target(base), None);

    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
    assert_eq!(
        ecam.mmio_target(base),
        Some(PcieMmioTarget {
            bdf: XHCI_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(
        ecam.mmio_target(base + 0x3fff).map(|target| target.offset),
        Some(0x3fff)
    );
    assert_eq!(ecam.mmio_target(base + u64::from(XHCI_BAR0_SIZE)), None);
}

#[test]
fn qemu_xhci_64bit_bar_decodes_high_mmio_after_command_enable() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
    let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
    let cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
    let base = machine::PCIE_MMIO_64.base;

    ecam.cfg_write(bar0, 4, base & 0xffff_ffff);
    ecam.cfg_write(bar1, 4, base >> 32);
    assert_eq!(ecam.mmio_target(base), None);

    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
    assert_eq!(ecam.mmio_target(base - 1), None);
    assert_eq!(
        ecam.mmio_target(base),
        Some(PcieMmioTarget {
            bdf: XHCI_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(
        ecam.mmio_target(base + 0x3fff).map(|target| target.offset),
        Some(0x3fff)
    );
    assert_eq!(ecam.mmio_target(base + u64::from(XHCI_BAR0_SIZE)), None);
}

#[test]
fn writes_to_empty_slots_are_dropped() {
    let mut ecam = PcieEcam::new();
    ecam.cfg_write(ecam_offset(0, 4, 0, REG_COMMAND_STATUS), 2, 0x7);
    // Still empty.
    assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 4), NO_DEVICE);
}

#[test]
fn command_register_is_writable_and_reads_back() {
    let mut ecam = PcieEcam::new();
    // Initially the command register is clear.
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2),
        0
    );
    // Enable memory space + bus master.
    let cmd = u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER);
    ecam.cfg_write(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2, cmd);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2),
        cmd
    );
    // Non-writable command bits (e.g. bit 0, I/O space) are masked off.
    ecam.cfg_write(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2, 0xFFFF);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 2),
        u64::from(CMD_WRITABLE_MASK)
    );
}

#[test]
fn status_high_word_is_not_clobbered_by_a_command_write() {
    let mut ecam = PcieEcam::new();
    // A 4-byte write to the command/status dword must only touch command.
    ecam.cfg_write(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 4, 0xFFFF_FFFF);
    let dword = ecam.cfg_read(ecam_offset(0, 0, 0, REG_COMMAND_STATUS), 4);
    assert_eq!(dword & 0xFFFF, u64::from(CMD_WRITABLE_MASK));
    // Host bridge has no cap list, so the status word stays zero.
    assert_eq!(dword >> 16, 0);
}

#[test]
fn host_bridge_bars_have_no_decode() {
    let ecam = PcieEcam::new();
    for i in 0..NUM_BARS {
        let reg = REG_BAR0 + (i as u16) * 4;
        assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 0, reg), 4), 0);
    }
}

#[test]
fn host_bridge_bar_sizing_returns_zero_for_unimplemented_bars() {
    let mut ecam = PcieEcam::new();
    // The host bridge has no BARs: the all-ones sizing probe reads back 0
    // (a zero size mask means "no region"), which firmware reads as "skip".
    ecam.cfg_write(ecam_offset(0, 0, 0, REG_BAR0), 4, 0xFFFF_FFFF);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 0, REG_BAR0), 4), 0);
}

#[test]
fn nvme_endpoint_reports_storage_class_and_bar0() {
    let ecam = PcieEcam::new();
    let vd = ecam.cfg_read(ecam_offset(0, 1, 0, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xFFFF, u64::from(NVME_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xFFFF, u64::from(NVME_DEVICE_ID));

    let rc = ecam.cfg_read(ecam_offset(0, 1, 0, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(NVME_CLASS_CODE));
    assert_eq!(rc & 0xFF, u64::from(NVME_REVISION));

    // BAR0 is a 64-bit memory BAR: unprogrammed it reads back only its
    // hardwired type bits (bit 2 = 64-bit), matching QEMU's NVMe endpoint.
    assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, REG_BAR0), 4), 0x4);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, REG_BAR0 + 4), 4), 0);
}

#[test]
fn nvme_endpoint_exposes_msix_capability() {
    let ecam = PcieEcam::new();
    let status = ecam.cfg_read(ecam_offset(0, 1, 0, REG_COMMAND_STATUS), 4) >> 16;
    assert_ne!(
        status & u64::from(STATUS_CAP_LIST),
        0,
        "NVMe endpoint must advertise a PCI capability list"
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, REG_CAP_PTR), 1),
        u64::from(NVME_MSIX_CAP_OFFSET)
    );

    let cap = u16::from(NVME_MSIX_CAP_OFFSET);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, cap), 1),
        u64::from(CAP_ID_MSIX)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, cap + 1), 1),
        u64::from(NVME_PM_CAP_OFFSET),
        "MSI-X capability must chain to the Power Management capability"
    );
    // Power Management capability (ID 0x01) chains onward to PCI Express.
    let pm = u16::from(NVME_PM_CAP_OFFSET);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, pm), 1), 0x01);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, pm + 1), 1),
        u64::from(NVME_PCIE_CAP_OFFSET),
        "Power Management capability must chain to PCI Express"
    );
    // The PCI Express capability (ID 0x10) that EDK2's NvmExpressDxe needs
    // terminates the list.
    let pcie = u16::from(NVME_PCIE_CAP_OFFSET);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, pcie), 1), 0x10);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, pcie + 1), 1),
        0,
        "PCI Express capability terminates the list"
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, cap + 2), 2),
        u64::from(NVME_MSIX_VECTOR_COUNT - 1),
        "MSI-X table-size field is encoded as count - 1"
    );
    assert_eq!(
        NVME_MSIX_VECTOR_COUNT, 9,
        "NVMe advertises one admin vector plus eight I/O vectors"
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, cap + 4), 4),
        u64::from(NVME_MSIX_TABLE_OFFSET)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, cap + 8), 4),
        u64::from(NVME_MSIX_PBA_OFFSET)
    );
    const _: () = assert!(
        NVME_MSIX_TABLE_OFFSET + (NVME_MSIX_VECTOR_COUNT as u32) * MsixCapability::ENTRY_BYTES
            <= NVME_BAR0_SIZE
    );
    const _: () = assert!(NVME_MSIX_PBA_OFFSET + 8 <= NVME_BAR0_SIZE);
}

#[test]
fn nvme_msix_enable_and_function_mask_bits_are_writable() {
    let mut ecam = PcieEcam::new();
    let control = u16::from(NVME_MSIX_CAP_OFFSET) + 2;

    assert_eq!(ecam.nvme_msix_control(), MsixFunctionControl::default());

    // The table-size bits are read-only; only function-mask and enable move.
    ecam.cfg_write(ecam_offset(0, 1, 0, control), 2, 0xffff);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, control), 2),
        u64::from(0xc000 | (NVME_MSIX_VECTOR_COUNT - 1))
    );
    assert_eq!(
        ecam.nvme_msix_control(),
        MsixFunctionControl {
            enabled: true,
            function_masked: true,
        }
    );

    ecam.cfg_write(ecam_offset(0, 1, 0, control + 1), 1, 0x00);
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 1, 0, control), 2),
        u64::from(NVME_MSIX_VECTOR_COUNT - 1),
        "sub-byte writes clear the writable MSI-X control bits"
    );
    assert_eq!(ecam.nvme_msix_control(), MsixFunctionControl::default());
}

#[test]
fn nvme_command_enables_bar0_mmio_decode() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 1, 0, REG_BAR0);
    let bar1 = ecam_offset(0, 1, 0, REG_BAR0 + 4);
    let cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);

    ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
    let readback = ecam.cfg_read(bar0, 4) as u32;
    // Bit 2 of a 64-bit memory BAR is the hardwired type indicator; mask
    // the low 4 bits before computing the aperture size.
    let size = (!(readback & !0xF)).wrapping_add(1);
    assert_eq!(size, NVME_BAR0_SIZE);
    assert_eq!(readback & 0x6, 0x4, "NVMe BAR0 must advertise 64-bit type");

    let base = machine::PCIE_MMIO_32.base as u32;
    ecam.cfg_write(bar0, 4, u64::from(base));
    ecam.cfg_write(bar1, 4, 0);
    assert_eq!(ecam.cfg_read(bar0, 4) & !0xF, u64::from(base));
    assert!(ecam.nvme_endpoint_state().bar0_assigned);
    assert!(!ecam.nvme_endpoint_state().command_memory_enabled);
    assert!(!ecam.nvme_endpoint_state().command_bus_master_enabled);
    assert_eq!(ecam.mmio_target(machine::PCIE_MMIO_32.base), None);

    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
    assert!(ecam.nvme_endpoint_state().command_memory_enabled);
    assert!(ecam.nvme_endpoint_state().command_bus_master_enabled);
    assert_eq!(
        ecam.mmio_target(machine::PCIE_MMIO_32.base),
        Some(PcieMmioTarget {
            bdf: NVME_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(
        ecam.mmio_target(machine::PCIE_MMIO_32.base + 0x1234)
            .map(|t| t.offset),
        Some(0x1234)
    );
    assert_eq!(
        ecam.mmio_target(machine::PCIE_MMIO_32.base + u64::from(NVME_BAR0_SIZE)),
        None
    );
}

#[test]
fn mmio_target_mru_hits_same_bar_and_invalidates_on_config_write() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 1, 0, REG_BAR0);
    let bar1 = ecam_offset(0, 1, 0, REG_BAR0 + 4);
    let cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);
    let base = machine::PCIE_MMIO_32.base;

    ecam.cfg_write(bar0, 4, base);
    ecam.cfg_write(bar1, 4, 0);
    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
    assert_eq!(ecam.mmio_mru.get(), None);

    assert_eq!(
        ecam.mmio_target(base),
        Some(PcieMmioTarget {
            bdf: NVME_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    let cached = ecam.mmio_mru.get().expect("mmio target cache populated");
    assert_eq!(cached.base, base);
    assert_eq!(cached.end, base + u64::from(NVME_BAR0_SIZE));
    assert_eq!(ecam.mmio_target(base - 0x10), None);

    assert_eq!(
        ecam.mmio_target(base + 0x40),
        Some(PcieMmioTarget {
            bdf: NVME_BDF,
            bar_index: 0,
            offset: 0x40,
        })
    );
    assert_eq!(ecam.mmio_mru.get(), Some(cached));

    ecam.cfg_write(cmd, 2, 0);
    assert_eq!(ecam.mmio_mru.get(), None);
    assert_eq!(ecam.mmio_target(base), None);
}

#[test]
fn xhci_command_enable_does_not_satisfy_nvme_command_or_decode() {
    let mut ecam = PcieEcam::new();
    let nvme_bar0 = ecam_offset(0, 1, 0, REG_BAR0);
    let nvme_cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);
    let xhci_bar0 = ecam_offset(0, 2, 0, REG_BAR0);
    let xhci_bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
    let xhci_cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
    let nvme_base = machine::PCIE_MMIO_32.base;
    let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

    // Given: NVMe has a BAR0 base, while only xHCI has command bits enabled.
    ecam.cfg_write(nvme_bar0, 4, nvme_base);
    ecam.cfg_write(xhci_bar0, 4, xhci_base);
    ecam.cfg_write(xhci_bar1, 4, 0);
    ecam.cfg_write(xhci_cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

    // Then: xHCI enablement remains separate from the NVMe endpoint.
    let nvme_state = ecam.nvme_endpoint_state();
    assert!(nvme_state.bar0_assigned);
    assert!(!nvme_state.command_memory_enabled);
    assert!(!nvme_state.command_bus_master_enabled);
    assert_eq!(ecam.mmio_target(nvme_base), None);
    assert_eq!(
        ecam.mmio_target(xhci_base),
        Some(PcieMmioTarget {
            bdf: XHCI_BDF,
            bar_index: 0,
            offset: 0,
        })
    );

    // When: NVMe command bits are written, its own BAR starts decoding.
    ecam.cfg_write(nvme_cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

    // Then: the NVMe target is enabled by NVMe's command register only.
    assert_eq!(
        ecam.mmio_target(nvme_base),
        Some(PcieMmioTarget {
            bdf: NVME_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
}

#[test]
fn nvme_bar0_sizing_probe_does_not_decode_after_command_enable() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 1, 0, REG_BAR0);
    let cmd = ecam_offset(0, 1, 0, REG_COMMAND_STATUS);

    // Given: firmware is probing BAR0 size, not assigning a real base.
    ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
    let sizing_readback = ecam.cfg_read(bar0, 4);
    let sizing_probe_base = sizing_readback & !0xF;
    assert!(!ecam.nvme_endpoint_state().bar0_assigned);

    // When: command memory/bus-master bits are enabled while the sizing
    // latch is still present.
    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

    // Then: the sizing value is still not an assigned BAR and must not
    // decode as the NVMe MMIO aperture.
    assert!(!ecam.nvme_endpoint_state().bar0_assigned);
    assert_eq!(ecam.mmio_target(sizing_probe_base), None);
}

#[test]
fn bar_sizing_returns_a_power_of_two_mask() {
    // Exercise the BAR sizing protocol directly: a 64 KiB 32-bit memory BAR.
    let mut bar = Bar::memory32(0x1_0000);
    // Write all-ones, read back the size mask.
    bar.write(0xFFFF_FFFF);
    let readback = bar.read();
    // Firmware computes size as `!(readback & !0xF) + 1` for a memory BAR.
    let size = (!(readback & !0xF)).wrapping_add(1);
    assert_eq!(size, 0x1_0000);
    // The mask is a contiguous run of high ones => size is a power of two.
    assert!(size.is_power_of_two());
    // Programming a base keeps only the address bits above the size.
    bar.write(0x1234_5678);
    assert_eq!(bar.read() & !0xF, 0x1234_0000);
}

#[test]
fn msix_capability_encodes_size_bir_and_offsets() {
    let cap = MsixCapability::new(8, 0, 0x0000, 0x0800);
    // Message control encodes table_size - 1 in the low 11 bits.
    assert_eq!(cap.message_control(), 7);
    // Table/PBA dwords pack the BIR into the low 3 bits.
    assert_eq!(cap.table_offset_bir() & 0x7, 0);
    assert_eq!(cap.table_offset_bir() & !0x7, 0x0000);
    assert_eq!(cap.pba_offset_bir() & !0x7, 0x0800);
    // Table occupies 8 entries * 16 bytes.
    assert_eq!(cap.table_byte_size(), 8 * 16);

    let bytes = cap.to_bytes(0x00);
    assert_eq!(bytes[0], CAP_ID_MSIX);
    assert_eq!(bytes[1], 0x00); // end of capability list
    assert_eq!(u16::from_le_bytes([bytes[2], bytes[3]]), 7);
    assert_eq!(
        u32::from_le_bytes([bytes[4], bytes[5], bytes[6], bytes[7]]),
        cap.table_offset_bir()
    );
    assert_eq!(
        u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]),
        cap.pba_offset_bir()
    );
}

#[test]
fn msix_capability_supports_split_table_and_pba_birs() {
    let cap = MsixCapability::with_birs(2048, 2, 0x1000, 4, 0x2000);
    assert_eq!(cap.message_control(), 2047);
    assert_eq!(cap.table_offset_bir() & 0x7, 2);
    assert_eq!(cap.pba_offset_bir() & 0x7, 4);
}

#[test]
fn hda_endpoint_is_opt_in_and_matches_ich6_pci_contract() {
    let ecam = PcieEcam::new();
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_VENDOR_DEVICE), 4),
        NO_DEVICE
    );

    let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
        hda_present: true,
        ..PcieEcamConfig::default()
    });
    let identity = ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_VENDOR_DEVICE), 4);
    assert_eq!(identity, 0x2668_8086);
    let class = ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_REVISION_CLASS), 4);
    assert_eq!(class >> 8, u64::from(HDA_CLASS_CODE));
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_SUBSYSTEM_IDS), 4),
        (u64::from(HDA_SUBSYSTEM_ID) << 16) | u64::from(HDA_SUBSYSTEM_VENDOR_ID)
    );
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_INTERRUPT_LINE_PIN), 4),
        0x0,
        "HDA advertises no INTx pin (MSI-only): our platform has no _PRT INTx routing"
    );
    let status = ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_COMMAND_STATUS), 4);
    assert_ne!(status & (u64::from(STATUS_CAP_LIST) << 16), 0);
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, REG_CAP_PTR), 1),
        u64::from(HDA_MSI_CAP_OFFSET)
    );
    let msi = u16::from(HDA_MSI_CAP_OFFSET);
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, msi), 1),
        u64::from(CAP_ID_MSI)
    );
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, msi + 2), 2),
        0x0080,
        "64-bit address capable, one vector, MSI initially disabled"
    );

    let bar = bdf_ecam_offset(HDA_BDF, REG_BAR0);
    ecam.cfg_write(bar, 4, 0xffff_ffff);
    let mask = ecam.cfg_read(bar, 4) as u32;
    assert_eq!(mask & 0xf, 0, "HDA BAR0 is 32-bit non-prefetchable MMIO");
    assert_eq!((!(mask & !0xf)).wrapping_add(1), HDA_BAR0_SIZE);

    let bar1 = bdf_ecam_offset(HDA_BDF, REG_BAR0 + 4);
    ecam.cfg_write(bar1, 4, 0xffff_ffff);
    assert_eq!(ecam.cfg_read(bar1, 4), 0, "HDA BAR1 is unimplemented");

    let base = crate::machine::PCIE_MMIO_32.base + 0x70_000;
    ecam.cfg_write(bar, 4, base);
    ecam.cfg_write(
        bdf_ecam_offset(HDA_BDF, REG_COMMAND_STATUS),
        2,
        u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
    );
    assert_eq!(
        ecam.mmio_target(base),
        Some(PcieMmioTarget {
            bdf: HDA_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(
        ecam.mmio_target(base + u64::from(HDA_BAR0_SIZE)),
        None,
        "no HDA MSI table BAR may decode after BAR0"
    );

    let message_address = 0x1234_5678_8088_4000u64;
    ecam.cfg_write(
        bdf_ecam_offset(HDA_BDF, msi + 4),
        4,
        message_address as u32 as u64,
    );
    ecam.cfg_write(bdf_ecam_offset(HDA_BDF, msi + 8), 4, message_address >> 32);
    ecam.cfg_write(bdf_ecam_offset(HDA_BDF, msi + 12), 2, 0x61);
    ecam.cfg_write(bdf_ecam_offset(HDA_BDF, msi + 2), 2, 0xffff);
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, msi + 2), 2),
        0x0081,
        "only MSI Enable is writable in Message Control"
    );
    assert_eq!(
        ecam.hda_msi_config(),
        HdaMsiConfig {
            enabled: true,
            address: message_address,
            data: 0x61,
        }
    );

    ecam.cfg_write(bdf_ecam_offset(HDA_BDF, msi + 4), 1, 0xff);
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(HDA_BDF, msi + 4), 1),
        0xfc,
        "Message Address Low bits 1:0 are read-only zero"
    );
}

#[test]
#[should_panic(expected = "table size")]
fn msix_rejects_an_out_of_range_table_size() {
    let _ = MsixCapability::new(0, 0, 0, 0);
}

#[test]
#[should_panic(expected = "8-byte aligned")]
fn msix_rejects_a_misaligned_offset() {
    let _ = MsixCapability::new(4, 0, 0x4, 0x800);
}
