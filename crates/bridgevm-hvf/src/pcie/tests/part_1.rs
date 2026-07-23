//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::machine;

#[test]
fn ecam_offset_decodes_into_bdf_reg() {
    let off = ecam_offset(0x12, 0x1a, 0x5, 0x3c);
    let addr = CfgAddr::from_ecam_offset(off);
    assert_eq!(addr.bus, 0x12);
    assert_eq!(addr.device, 0x1a);
    assert_eq!(addr.function, 0x5);
    assert_eq!(addr.reg, 0x3c);
    assert_eq!(addr.bdf(), (0x12, 0x1a, 0x5));
}

#[test]
fn window_matches_the_machine_map() {
    assert_eq!(PcieEcam::window(), machine::PCIE_ECAM);
    assert_eq!(PcieEcam::window().base, 0x40_1000_0000);
}

#[test]
fn host_bridge_reports_vendor_and_device_id() {
    let ecam = PcieEcam::new();
    // 4-byte read of reg 0 gives device:vendor packed high:low.
    let vd = ecam.cfg_read(ecam_offset(0, 0, 0, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xFFFF, u64::from(HOST_BRIDGE_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xFFFF, u64::from(HOST_BRIDGE_DEVICE_ID));
    // 2-byte reads pick out the individual fields.
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 0, 0, 0x00), 2),
        u64::from(HOST_BRIDGE_VENDOR_ID)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(0, 0, 0, 0x02), 2),
        u64::from(HOST_BRIDGE_DEVICE_ID)
    );
    assert!(ecam.host_bridge_present());
}

#[test]
fn host_bridge_reports_host_bridge_class_and_header_type() {
    let ecam = PcieEcam::new();
    // Class code lives in the upper 24 bits of the revision/class dword.
    let rc = ecam.cfg_read(ecam_offset(0, 0, 0, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(HOST_BRIDGE_CLASS_CODE));
    assert_eq!(rc & 0xFF, u64::from(HOST_BRIDGE_REVISION));
    // Header type byte (offset 0x0e) is type-0.
    let header = ecam.cfg_read(ecam_offset(0, 0, 0, 0x0e), 1);
    assert_eq!(header, u64::from(HEADER_TYPE_ENDPOINT));
}

#[test]
fn empty_slot_reads_all_ones() {
    let ecam = PcieEcam::new();
    assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 4), NO_DEVICE);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 2), 0xFFFF);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 4, 0, 0x00), 1), 0xFF);
    // A different function of device 0 is also empty.
    assert_eq!(ecam.cfg_read(ecam_offset(0, 0, 1, 0x00), 4), NO_DEVICE);
    // A non-zero bus is empty.
    assert_eq!(ecam.cfg_read(ecam_offset(1, 0, 0, 0x00), 4), NO_DEVICE);
}

#[test]
fn boot_media_config_space_bytes_stay_byte_identical() {
    let ecam = PcieEcam::new();
    let mut expected = [0u8; 0x100];
    expected[0x00..0x04].copy_from_slice(&[0xf4, 0x1a, 0x01, 0x10]);
    expected[0x04..0x08].copy_from_slice(&[0x00, 0x00, 0x10, 0x00]);
    expected[0x08..0x0c].copy_from_slice(&[0x00, 0x00, 0x00, 0x01]);
    expected[0x2c..0x30].copy_from_slice(&[0xf4, 0x1a, 0x02, 0x00]);
    expected[0x34] = 0x40;
    expected[0x40..0x50].copy_from_slice(&[
        0x09, 0x50, 0x10, 0x01, 0x04, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00,
        0x00,
    ]);
    expected[0x50..0x64].copy_from_slice(&[
        0x09, 0x64, 0x14, 0x02, 0x04, 0x00, 0x00, 0x00, 0x00, 0x30, 0x00, 0x00, 0x00, 0x10, 0x00,
        0x00, 0x04, 0x00, 0x00, 0x00,
    ]);
    expected[0x64..0x74].copy_from_slice(&[
        0x09, 0x74, 0x10, 0x03, 0x04, 0x00, 0x00, 0x00, 0x00, 0x10, 0x00, 0x00, 0x00, 0x10, 0x00,
        0x00,
    ]);
    expected[0x74..0x84].copy_from_slice(&[
        0x09, 0x84, 0x10, 0x04, 0x04, 0x00, 0x00, 0x00, 0x00, 0x20, 0x00, 0x00, 0x00, 0x10, 0x00,
        0x00,
    ]);
    expected[0x84..0x90].copy_from_slice(&[
        0x11, 0x00, 0x01, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x00,
    ]);

    assert_eq!(
        read_config_bytes(&ecam, VIRTIO_BLK_BDF, expected.len()),
        expected
    );
}

#[test]
fn boot_media_endpoint_reports_qemu_oracle_identity_at_00_03_0() {
    let ecam = PcieEcam::new();
    let (bus, dev, func) = VIRTIO_BLK_BDF;

    let vd = ecam.cfg_read(ecam_offset(bus, dev, func, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xFFFF, u64::from(VIRTIO_BLK_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xFFFF, u64::from(VIRTIO_BLK_DEVICE_ID));

    let rc = ecam.cfg_read(ecam_offset(bus, dev, func, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(VIRTIO_BLK_CLASS_CODE));
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, 0x0e), 1),
        u64::from(HEADER_TYPE_ENDPOINT)
    );
}

#[test]
fn boot_media_endpoint_reports_qemu_oracle_subsystem_id() {
    let ecam = PcieEcam::new();
    let (bus, dev, func) = VIRTIO_BLK_BDF;

    let subsystem = ecam.cfg_read(ecam_offset(bus, dev, func, REG_SUBSYSTEM_IDS), 4);
    assert_eq!(
        subsystem & 0xFFFF,
        u64::from(VIRTIO_BLK_SUBSYSTEM_VENDOR_ID)
    );
    assert_eq!(
        (subsystem >> 16) & 0xFFFF,
        u64::from(VIRTIO_BLK_SUBSYSTEM_ID)
    );
}

#[test]
fn boot_media_given_bars_when_sizing_then_matches_qemu_oracle_shape() {
    let mut ecam = PcieEcam::new();
    let (bus, dev, func) = VIRTIO_BLK_BDF;

    // Given: QEMU's virtio-blk-pci exposes BAR0 as 0x80 bytes of PIO.
    let bar0 = ecam_offset(bus, dev, func, REG_BAR0);
    ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
    let bar0_readback = ecam.cfg_read(bar0, 4) as u32;
    let bar0_size = (!(bar0_readback & !0x3)).wrapping_add(1);
    assert_eq!(bar0_readback & 0x1, 0x1, "BAR0 must be an I/O BAR");
    assert_eq!(bar0_size, 0x80);

    // Given: BAR1 is the 32-bit memory aperture used for MSI-X.
    let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
    ecam.cfg_write(bar1, 4, 0xFFFF_FFFF);
    let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
    assert_eq!(bar1_readback & 0xF, 0, "BAR1 must be 32-bit memory");
    assert_eq!((!(bar1_readback & !0xF)).wrapping_add(1), 0x1000);

    // Then: BAR4 is the modern virtio MMIO transport block, sized 0x4000.
    let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
    ecam.cfg_write(bar4, 4, 0xFFFF_FFFF);
    let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
    assert_eq!(bar4_readback & 0xF, 0, "BAR4 must be 32-bit memory");
    assert_eq!((!(bar4_readback & !0xF)).wrapping_add(1), 0x4000);
}

#[test]
fn boot_media_given_bars_when_command_bits_change_then_pio_and_mmio_decode_separately() {
    let mut ecam = PcieEcam::new();
    let (bus, dev, func) = VIRTIO_BLK_BDF;
    let bar0 = ecam_offset(bus, dev, func, REG_BAR0);
    let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
    let cmd = ecam_offset(bus, dev, func, REG_COMMAND_STATUS);
    let pio_base = 0xC000;
    let mmio_base = machine::PCIE_MMIO_32.base + 0x1_0000;

    // Given: firmware programmed both BAR0 PIO and BAR4 MMIO bases.
    ecam.cfg_write(bar0, 4, pio_base);
    ecam.cfg_write(bar4, 4, mmio_base);
    assert_eq!(ecam.pio_target(pio_base), None);
    assert_eq!(ecam.mmio_target(mmio_base), None);

    // When: only I/O space is enabled, only BAR0 decodes.
    ecam.cfg_write(cmd, 2, u64::from(CMD_IO_SPACE));
    assert_eq!(
        ecam.pio_target(pio_base),
        Some(PciePioTarget {
            bdf: VIRTIO_BLK_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(ecam.mmio_target(mmio_base), None);

    // When: only memory space is enabled, only BAR4 decodes.
    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
    assert_eq!(ecam.pio_target(pio_base), None);
    assert_eq!(
        ecam.mmio_target(mmio_base),
        Some(PcieMmioTarget {
            bdf: VIRTIO_BLK_BDF,
            bar_index: 4,
            offset: 0,
        })
    );
}

#[test]
fn virtio_net_endpoint_is_absent_by_default_and_gated_on() {
    let ecam = PcieEcam::new();
    assert_eq!(
        ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_VENDOR_DEVICE), 4),
        NO_DEVICE
    );

    let ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        ..PcieEcamConfig::default()
    });
    let vd = ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xffff, u64::from(VIRTIO_NET_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xffff, u64::from(VIRTIO_NET_DEVICE_ID));
    let rc = ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(VIRTIO_NET_CLASS_CODE));
    assert_eq!(rc & 0xff, u64::from(VIRTIO_NET_REVISION));
    let subsystem = ecam.cfg_read(bdf_ecam_offset(VIRTIO_NET_BDF, REG_SUBSYSTEM_IDS), 4);
    assert_eq!(
        subsystem & 0xffff,
        u64::from(VIRTIO_NET_SUBSYSTEM_VENDOR_ID)
    );
    assert_eq!(
        (subsystem >> 16) & 0xffff,
        u64::from(VIRTIO_NET_SUBSYSTEM_ID)
    );
}

#[test]
fn virtio_net_modern_bars_and_capabilities_match_stage1_shape() {
    let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        ..PcieEcamConfig::default()
    });
    let (bus, dev, func) = VIRTIO_NET_BDF;
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, REG_CAP_PTR), 1),
        0x40
    );

    let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
    ecam.cfg_write(bar1, 4, 0xffff_ffff);
    let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
    assert_eq!(bar1_readback & 0xf, 0);
    assert_eq!(
        (!(bar1_readback & !0xf)).wrapping_add(1),
        VIRTIO_NET_BAR1_SIZE
    );

    let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
    ecam.cfg_write(bar4, 4, 0xffff_ffff);
    let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
    assert_eq!(bar4_readback & 0xf, 0);
    assert_eq!(
        (!(bar4_readback & !0xf)).wrapping_add(1),
        VIRTIO_NET_BAR4_SIZE
    );

    let msix = u16::from(VIRTIO_NET_MSIX_CAP_OFFSET);
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix), 1),
        u64::from(CAP_ID_MSIX)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 2), 2),
        u64::from(VIRTIO_NET_MSIX_VECTOR_COUNT - 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 4), 4),
        u64::from(VIRTIO_NET_MSIX_TABLE_OFFSET | 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 8), 4),
        u64::from(VIRTIO_NET_MSIX_PBA_OFFSET | 1)
    );
}

#[test]
fn virtio_net_bar1_and_bar4_decode_after_command_enable() {
    let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        ..PcieEcamConfig::default()
    });
    let (bus, dev, func) = VIRTIO_NET_BDF;
    let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
    let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
    let cmd = ecam_offset(bus, dev, func, REG_COMMAND_STATUS);
    let bar1_base = machine::PCIE_MMIO_32.base + 0x4_0000;
    let bar4_base = machine::PCIE_MMIO_32.base + 0x5_0000;

    ecam.cfg_write(bar1, 4, bar1_base);
    ecam.cfg_write(bar4, 4, bar4_base);
    assert_eq!(ecam.mmio_target(bar1_base), None);
    assert_eq!(ecam.mmio_target(bar4_base), None);

    ecam.cfg_write(cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));
    assert_eq!(
        ecam.mmio_target(bar1_base),
        Some(PcieMmioTarget {
            bdf: VIRTIO_NET_BDF,
            bar_index: 1,
            offset: 0,
        })
    );
    assert_eq!(
        ecam.mmio_target(bar4_base),
        Some(PcieMmioTarget {
            bdf: VIRTIO_NET_BDF,
            bar_index: 4,
            offset: 0,
        })
    );
}

#[test]
fn virtio_gpu_endpoint_is_absent_by_default_and_gated_on_without_regressing_net_or_blk() {
    let baseline = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        ..PcieEcamConfig::default()
    });
    assert_eq!(
        baseline.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE), 4),
        NO_DEVICE
    );
    let baseline_blk = read_config_bytes(&baseline, VIRTIO_BLK_BDF, 256);
    let baseline_net = read_config_bytes(&baseline, VIRTIO_NET_BDF, 256);

    let ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        virtio_gpu_present: true,
        ..PcieEcamConfig::default()
    });
    assert_eq!(read_config_bytes(&ecam, VIRTIO_BLK_BDF, 256), baseline_blk);
    assert_eq!(read_config_bytes(&ecam, VIRTIO_NET_BDF, 256), baseline_net);

    let vd = ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xffff, u64::from(VIRTIO_GPU_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xffff, u64::from(VIRTIO_GPU_DEVICE_ID));
    let rc = ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(VIRTIO_GPU_CLASS_CODE));
    assert_eq!(rc & 0xff, u64::from(VIRTIO_GPU_REVISION));
    let subsystem = ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_SUBSYSTEM_IDS), 4);
    assert_eq!(
        subsystem & 0xffff,
        u64::from(VIRTIO_GPU_SUBSYSTEM_VENDOR_ID)
    );
    assert_eq!(
        (subsystem >> 16) & 0xffff,
        u64::from(VIRTIO_GPU_SUBSYSTEM_ID)
    );
}

#[test]
fn virtio_console_endpoint_is_absent_by_default_and_gated_on_without_regressing_other_virtio() {
    let baseline = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        virtio_gpu_present: true,
        ..PcieEcamConfig::default()
    });
    assert_eq!(
        baseline.cfg_read(bdf_ecam_offset(VIRTIO_CONSOLE_BDF, REG_VENDOR_DEVICE), 4),
        NO_DEVICE
    );
    let baseline_blk = read_config_bytes(&baseline, VIRTIO_BLK_BDF, 256);
    let baseline_net = read_config_bytes(&baseline, VIRTIO_NET_BDF, 256);
    let baseline_gpu = read_config_bytes(&baseline, VIRTIO_GPU_BDF, 256);

    let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        virtio_gpu_present: true,
        virtio_console_present: true,
        ..PcieEcamConfig::default()
    });
    assert_eq!(read_config_bytes(&ecam, VIRTIO_BLK_BDF, 256), baseline_blk);
    assert_eq!(read_config_bytes(&ecam, VIRTIO_NET_BDF, 256), baseline_net);
    assert_eq!(read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256), baseline_gpu);

    let (bus, dev, func) = VIRTIO_CONSOLE_BDF;
    let vd = ecam.cfg_read(ecam_offset(bus, dev, func, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xffff, u64::from(VIRTIO_CONSOLE_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xffff, u64::from(VIRTIO_CONSOLE_DEVICE_ID));
    let rc = ecam.cfg_read(ecam_offset(bus, dev, func, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(VIRTIO_CONSOLE_CLASS_CODE));
    assert_eq!(rc & 0xff, u64::from(VIRTIO_CONSOLE_REVISION));
    let subsystem = ecam.cfg_read(ecam_offset(bus, dev, func, REG_SUBSYSTEM_IDS), 4);
    assert_eq!(
        subsystem & 0xffff,
        u64::from(VIRTIO_CONSOLE_SUBSYSTEM_VENDOR_ID)
    );
    assert_eq!(
        (subsystem >> 16) & 0xffff,
        u64::from(VIRTIO_CONSOLE_SUBSYSTEM_ID)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, REG_CAP_PTR), 1),
        0x40
    );

    let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
    ecam.cfg_write(bar1, 4, 0xffff_ffff);
    let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
    assert_eq!(bar1_readback & 0xf, 0);
    assert_eq!(
        (!(bar1_readback & !0xf)).wrapping_add(1),
        VIRTIO_CONSOLE_BAR1_SIZE
    );

    let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
    ecam.cfg_write(bar4, 4, 0xffff_ffff);
    let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
    assert_eq!(bar4_readback & 0xf, 0);
    assert_eq!(
        (!(bar4_readback & !0xf)).wrapping_add(1),
        VIRTIO_CONSOLE_BAR4_SIZE
    );

    let msix = u16::from(VIRTIO_CONSOLE_MSIX_CAP_OFFSET);
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix), 1),
        u64::from(CAP_ID_MSIX)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 2), 2),
        u64::from(VIRTIO_CONSOLE_MSIX_VECTOR_COUNT - 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 4), 4),
        u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET | 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 8), 4),
        u64::from(VIRTIO_CONSOLE_MSIX_PBA_OFFSET | 1)
    );
    assert!(cap_chain_contains_vendor_cfg_type(
        &ecam,
        VIRTIO_CONSOLE_BDF,
        1
    ));
    assert!(cap_chain_contains_vendor_cfg_type(
        &ecam,
        VIRTIO_CONSOLE_BDF,
        2
    ));
    assert!(cap_chain_contains_vendor_cfg_type(
        &ecam,
        VIRTIO_CONSOLE_BDF,
        3
    ));
    assert!(cap_chain_contains_vendor_cfg_type(
        &ecam,
        VIRTIO_CONSOLE_BDF,
        4
    ));
}

#[test]
fn virtio_gpu_pci_device_id_defaults_and_can_be_overridden_without_changing_device_shape() {
    let default_ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        virtio_gpu_present: true,
        ..PcieEcamConfig::default()
    });
    let override_ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_net_present: true,
        virtio_gpu_present: true,
        virtio_gpu_pci_device_id: 0x10f7,
        ..PcieEcamConfig::default()
    });

    assert_eq!(
        default_ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE + 2), 2),
        u64::from(VIRTIO_GPU_DEVICE_ID)
    );
    assert_eq!(
        override_ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, REG_VENDOR_DEVICE + 2), 2),
        0x10f7
    );

    let mut default_gpu = read_config_bytes(&default_ecam, VIRTIO_GPU_BDF, 256);
    let mut override_gpu = read_config_bytes(&override_ecam, VIRTIO_GPU_BDF, 256);
    assert_eq!(
        &default_gpu[0..2],
        &override_gpu[0..2],
        "virtio-gpu vendor id must not change"
    );
    assert_ne!(&default_gpu[2..4], &override_gpu[2..4]);
    default_gpu[2..4].copy_from_slice(&[0, 0]);
    override_gpu[2..4].copy_from_slice(&[0, 0]);
    assert_eq!(
        default_gpu, override_gpu,
        "only the virtio-gpu PCI device-id field may differ"
    );

    for bdf in [NVME_BDF, XHCI_BDF, VIRTIO_BLK_BDF, VIRTIO_NET_BDF] {
        assert_eq!(
            read_config_bytes(&default_ecam, bdf, 256),
            read_config_bytes(&override_ecam, bdf, 256),
            "non-GPU PCI function {bdf:?} changed"
        );
    }
}

#[test]
fn virtio_gpu_modern_bars_and_capabilities_match_stage_g1_shape() {
    assert_eq!(VIRTIO_GPU_MSIX_VECTOR_COUNT, 3);
    let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_gpu_present: true,
        ..PcieEcamConfig::default()
    });
    let (bus, dev, func) = VIRTIO_GPU_BDF;
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, REG_CAP_PTR), 1),
        0x40
    );

    let bar1 = ecam_offset(bus, dev, func, REG_BAR0 + 4);
    ecam.cfg_write(bar1, 4, 0xffff_ffff);
    let bar1_readback = ecam.cfg_read(bar1, 4) as u32;
    assert_eq!(
        (!(bar1_readback & !0xf)).wrapping_add(1),
        VIRTIO_GPU_BAR1_SIZE
    );

    let bar4 = ecam_offset(bus, dev, func, REG_BAR0 + 4 * 4);
    ecam.cfg_write(bar4, 4, 0xffff_ffff);
    let bar4_readback = ecam.cfg_read(bar4, 4) as u32;
    assert_eq!(
        (!(bar4_readback & !0xf)).wrapping_add(1),
        VIRTIO_GPU_BAR4_SIZE
    );

    let msix = u16::from(VIRTIO_GPU_MSIX_CAP_OFFSET);
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix), 1),
        u64::from(CAP_ID_MSIX)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 2), 2),
        u64::from(VIRTIO_GPU_MSIX_VECTOR_COUNT - 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 4), 4),
        u64::from(VIRTIO_GPU_MSIX_TABLE_OFFSET | 1)
    );
    assert_eq!(
        ecam.cfg_read(ecam_offset(bus, dev, func, msix + 8), 4),
        u64::from(VIRTIO_GPU_MSIX_PBA_OFFSET | 1)
    );

    let bar1_base = machine::PCIE_MMIO_32.base + 0x6_0000;
    let bar4_base = machine::PCIE_MMIO_32.base + 0x7_0000;
    ecam.cfg_write(bar1, 4, bar1_base);
    ecam.cfg_write(bar4, 4, bar4_base);
    ecam.cfg_write(
        ecam_offset(bus, dev, func, REG_COMMAND_STATUS),
        2,
        u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER),
    );
    assert_eq!(
        ecam.mmio_target(bar1_base),
        Some(PcieMmioTarget {
            bdf: VIRTIO_GPU_BDF,
            bar_index: 1,
            offset: 0,
        })
    );
    assert_eq!(
        ecam.mmio_target(bar4_base),
        Some(PcieMmioTarget {
            bdf: VIRTIO_GPU_BDF,
            bar_index: 4,
            offset: 0,
        })
    );
}

#[test]
fn virtio_gpu_2d_config_has_no_host_visible_bar_or_shared_memory_capability() {
    let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
        virtio_gpu_present: true,
        virtio_gpu_3d_enabled: false,
        ..PcieEcamConfig::default()
    });
    let before = read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256);
    let bar2 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 2);
    ecam.cfg_write(bar2, 4, 0xffff_ffff);
    assert_eq!(ecam.cfg_read(bar2, 4), 0);
    assert_eq!(ecam.virtio_gpu_host_visible_bar_size(), None);
    assert!(!cap_chain_contains_vendor_cfg_type(
        &ecam,
        VIRTIO_GPU_BDF,
        8
    ));

    let after = read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256);
    assert_eq!(
        before, after,
        "BAR2 sizing on the 2D shape must not mutate config bytes"
    );
}

#[test]
fn virtio_gpu_3d_exposes_prefetchable_bar2_and_host_visible_shm_capability() {
    with_hostmem_mib_env("1024", || {
        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_gpu_present: true,
            virtio_gpu_3d_enabled: true,
            ..PcieEcamConfig::default()
        });
        let bar2 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 2);
        let bar3 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 3);
        assert_eq!(ecam.cfg_read(bar2, 4), 0x0c);
        assert_eq!(ecam.cfg_read(bar3, 4), 0);
        ecam.cfg_write(bar2, 4, 0xffff_ffff);
        ecam.cfg_write(bar3, 4, 0xffff_ffff);
        assert_eq!(ecam.cfg_read(bar2, 4), 0xc000_000c);
        assert_eq!(ecam.cfg_read(bar3, 4), 0xffff_ffff);
        assert_eq!(
            ecam.virtio_gpu_host_visible_bar_size(),
            Some(VIRTIO_GPU_HOSTMEM_DEFAULT_SIZE)
        );

        let high_base = machine::PCIE_MMIO_64.base;
        ecam.cfg_write(bar2, 4, high_base & 0xffff_ffff);
        ecam.cfg_write(bar3, 4, high_base >> 32);
        assert_eq!(ecam.virtio_gpu_host_visible_bar_base(), Some(high_base));

        // Windows places this 64 MiB BAR at the top of the 40-bit root
        // aperture.  Its low dword equals the sizing mask, but it is still
        // a programmed address once the all-ones probe has ended.
        let top_base = machine::PCIE_MMIO_64.end() - VIRTIO_GPU_HOSTMEM_DEFAULT_SIZE;
        ecam.cfg_write(bar2, 4, top_base & 0xffff_ffff);
        ecam.cfg_write(bar3, 4, top_base >> 32);
        assert_eq!(ecam.virtio_gpu_host_visible_bar_base(), Some(top_base));

        let cap = find_vendor_cfg_type(&ecam, VIRTIO_GPU_BDF, 8).expect("shared-memory cap");
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 2), 1),
            24
        );
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 4), 1),
            2
        );
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 5), 1),
            u64::from(VIRTIO_GPU_SHM_ID_HOST_VISIBLE)
        );
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 8), 4),
            0
        );
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 12), 4),
            VIRTIO_GPU_HOSTMEM_DEFAULT_SIZE & 0xffff_ffff
        );
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 16), 4),
            0
        );
        assert_eq!(
            ecam.cfg_read(bdf_ecam_offset(VIRTIO_GPU_BDF, cap + 20), 4),
            0
        );
    });
}

#[test]
fn virtio_gpu_3d_with_zero_hostmem_omits_bar2_and_shared_memory_capability() {
    with_hostmem_mib_env("0", || {
        assert_eq!(parse_virtio_gpu_hostmem_size(), 0);
        let mut ecam = PcieEcam::new_with_config(PcieEcamConfig {
            virtio_gpu_present: true,
            virtio_gpu_3d_enabled: true,
            ..PcieEcamConfig::default()
        });
        let before = read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256);
        let bar2 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 2);
        let bar3 = bdf_ecam_offset(VIRTIO_GPU_BDF, REG_BAR0 + 4 * 3);
        ecam.cfg_write(bar2, 4, 0xffff_ffff);
        ecam.cfg_write(bar3, 4, 0xffff_ffff);
        assert_eq!(ecam.cfg_read(bar2, 4), 0);
        assert_eq!(ecam.cfg_read(bar3, 4), 0);
        assert_eq!(ecam.virtio_gpu_host_visible_bar_size(), None);
        assert!(!cap_chain_contains_vendor_cfg_type(
            &ecam,
            VIRTIO_GPU_BDF,
            8
        ));
        assert_eq!(
            before,
            read_config_bytes(&ecam, VIRTIO_GPU_BDF, 256),
            "zero-hostmem BAR2/BAR3 sizing must not mutate config bytes"
        );
    });
}

#[test]
fn bar_decode_ignores_addresses_below_programmed_base() {
    let mut ecam = PcieEcam::new();
    let (bus, dev, func) = VIRTIO_BLK_BDF;
    let pio_bar0 = ecam_offset(bus, dev, func, REG_BAR0);
    let pio_cmd = ecam_offset(bus, dev, func, REG_COMMAND_STATUS);
    let pio_base = 0xc000;

    ecam.cfg_write(pio_bar0, 4, pio_base);
    ecam.cfg_write(pio_cmd, 2, u64::from(CMD_IO_SPACE));

    assert_eq!(ecam.pio_target(pio_base - 1), None);
    assert_eq!(
        ecam.pio_target(pio_base),
        Some(PciePioTarget {
            bdf: VIRTIO_BLK_BDF,
            bar_index: 0,
            offset: 0,
        })
    );

    let xhci_bar0 = ecam_offset(0, 2, 0, REG_BAR0);
    let xhci_bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);
    let xhci_cmd = ecam_offset(0, 2, 0, REG_COMMAND_STATUS);
    let mmio_base = machine::PCIE_MMIO_32.base + 0x2_0000;

    ecam.cfg_write(xhci_bar0, 4, mmio_base);
    ecam.cfg_write(xhci_bar1, 4, 0);
    ecam.cfg_write(xhci_cmd, 2, u64::from(CMD_MEMORY_SPACE | CMD_BUS_MASTER));

    assert_eq!(ecam.mmio_target(mmio_base - 1), None);
    assert_eq!(
        ecam.mmio_target(mmio_base),
        Some(PcieMmioTarget {
            bdf: XHCI_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
}

#[test]
fn qemu_xhci_endpoint_reports_oracle_identity_at_00_02_0() {
    let ecam = PcieEcam::new();

    let vd = ecam.cfg_read(ecam_offset(0, 2, 0, REG_VENDOR_DEVICE), 4);
    assert_eq!(vd & 0xFFFF, u64::from(XHCI_VENDOR_ID));
    assert_eq!((vd >> 16) & 0xFFFF, u64::from(XHCI_DEVICE_ID));

    let rc = ecam.cfg_read(ecam_offset(0, 2, 0, REG_REVISION_CLASS), 4);
    assert_eq!(rc >> 8, u64::from(XHCI_CLASS_CODE));
    assert_eq!(rc & 0xFF, u64::from(XHCI_REVISION));

    let subsystem = ecam.cfg_read(ecam_offset(0, 2, 0, REG_SUBSYSTEM_IDS), 4);
    assert_eq!(subsystem & 0xFFFF, u64::from(XHCI_SUBSYSTEM_VENDOR_ID));
    assert_eq!((subsystem >> 16) & 0xFFFF, u64::from(XHCI_SUBSYSTEM_ID));
}
