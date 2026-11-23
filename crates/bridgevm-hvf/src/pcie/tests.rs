use super::*;
use crate::machine;
use std::sync::Mutex;
use std::sync::OnceLock;

/// Build a raw ECAM offset for a (bus, dev, fn, reg) tuple, the way the run
/// loop derives it from a guest fault address minus the window base.
fn ecam_offset(bus: u8, dev: u8, func: u8, reg: u16) -> u64 {
    (u64::from(bus) << SHIFT_BUS)
        | (u64::from(dev) << SHIFT_DEV)
        | (u64::from(func) << SHIFT_FN)
        | u64::from(reg)
}

fn bdf_ecam_offset(bdf: (u8, u8, u8), reg: u16) -> u64 {
    ecam_offset(bdf.0, bdf.1, bdf.2, reg)
}

fn read_config_bytes(ecam: &PcieEcam, bdf: (u8, u8, u8), len: usize) -> Vec<u8> {
    (0..len)
        .map(|reg| {
            u8::try_from(ecam.cfg_read(bdf_ecam_offset(bdf, reg as u16), 1))
                .expect("single-byte config read fits in u8")
        })
        .collect()
}

fn find_vendor_cfg_type(ecam: &PcieEcam, bdf: (u8, u8, u8), cfg_type: u8) -> Option<u16> {
    let mut cap = ecam.cfg_read(bdf_ecam_offset(bdf, REG_CAP_PTR), 1) as u8;
    for _ in 0..32 {
        if cap == 0 {
            return None;
        }
        let cap_id = ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap)), 1) as u8;
        let next = ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap) + 1), 1) as u8;
        if cap_id == 0x09
            && ecam.cfg_read(bdf_ecam_offset(bdf, u16::from(cap) + 3), 1) as u8 == cfg_type
        {
            return Some(u16::from(cap));
        }
        cap = next;
    }
    None
}

fn cap_chain_contains_vendor_cfg_type(ecam: &PcieEcam, bdf: (u8, u8, u8), cfg_type: u8) -> bool {
    find_vendor_cfg_type(ecam, bdf, cfg_type).is_some()
}

fn with_hostmem_mib_env<R>(value: &str, f: impl FnOnce() -> R) -> R {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    let _guard = LOCK.get_or_init(|| Mutex::new(())).lock().unwrap();
    let previous = std::env::var_os("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB");
    std::env::set_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", value);
    let result = f();
    match previous {
        Some(previous) => std::env::set_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", previous),
        None => std::env::remove_var("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB"),
    }
    result
}

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
fn boot_media_endpoint_reports_declared_identity_at_00_03_0() {
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
fn boot_media_endpoint_reports_declared_subsystem_id() {
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
fn boot_media_bars_report_the_declared_transport_shape() {
    let mut ecam = PcieEcam::new();
    let (bus, dev, func) = VIRTIO_BLK_BDF;

    // BAR0 is the 0x80-byte transitional virtio PCI I/O region.
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
fn xhci_endpoint_reports_declared_identity_at_00_02_0() {
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

#[test]
fn xhci_exposes_msix_and_pcie_capabilities() {
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
fn xhci_bar0_is_64bit_16k_memory_bar() {
    let mut ecam = PcieEcam::new();
    let bar0 = ecam_offset(0, 2, 0, REG_BAR0);
    let bar1 = ecam_offset(0, 2, 0, REG_BAR0 + 4);

    ecam.cfg_write(bar0, 4, 0xFFFF_FFFF);
    ecam.cfg_write(bar1, 4, 0xFFFF_FFFF);

    assert_eq!(ecam.cfg_read(bar0, 4), 0xffff_c004);
    assert_eq!(ecam.cfg_read(bar1, 4), 0xffff_ffff);
}

#[test]
fn xhci_64bit_bar_decodes_low_mmio_after_command_enable() {
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
fn xhci_64bit_bar_decodes_high_mmio_after_command_enable() {
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

    // PCI requires the hardwired 64-bit memory type bit to survive while BAR0
    // is unprogrammed.
    assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, REG_BAR0), 4), 0x4);
    assert_eq!(ecam.cfg_read(ecam_offset(0, 1, 0, REG_BAR0 + 4), 4), 0);
}

#[test]
fn nvme_endpoint_exposes_msix_capability() {
    let mut ecam = PcieEcam::new();
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
    // PowerState follows guest writes (Windows writes D3hot at shutdown and
    // polls for it); unsupported D1/D2 and D3->D2-style moves are ignored, and
    // the rest of PMCSR (No_Soft_Reset) stays.
    let pmcsr = ecam_offset(0, 1, 0, pm + 4);
    assert_eq!(ecam.cfg_read(pmcsr, 2), 0x0008, "PMCSR starts in D0");
    ecam.cfg_write(pmcsr, 2, 0x0001);
    assert_eq!(ecam.cfg_read(pmcsr, 2), 0x0008, "D1 is not supported");
    ecam.cfg_write(pmcsr, 2, 0x0003);
    assert_eq!(ecam.cfg_read(pmcsr, 2), 0x000b, "D3hot reads back");
    ecam.cfg_write(pmcsr, 2, 0x0002);
    assert_eq!(ecam.cfg_read(pmcsr, 2), 0x000b, "no D3hot -> D2");
    ecam.cfg_write(pmcsr, 2, 0x0000);
    assert_eq!(ecam.cfg_read(pmcsr, 2), 0x0008, "D0 again");
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
