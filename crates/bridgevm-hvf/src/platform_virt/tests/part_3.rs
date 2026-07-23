//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::acpi::ACPI_LOADER_FILE;
use crate::acpi::ACPI_RSDP_FILE;
use crate::acpi::ACPI_TABLE_FILE;
use crate::fwcfg::GuestMemoryMut;
use crate::fwcfg::DMA_CTL_SELECT;
use crate::fwcfg::DMA_CTL_WRITE;
use crate::machine;
use crate::msix::MsixMessage;
use crate::ramfb::RamfbConfig;
use crate::ramfb::DRM_FORMAT_XRGB8888;
use crate::ramfb::RAMFB_CONFIG_SIZE;
use crate::smbios::SMBIOS_ANCHOR_FILE;
use crate::smbios::SMBIOS_TABLE_FILE;
use std::time::Duration;
use std::time::Instant;

#[test]
fn generated_acpi_tables_are_registered_by_default() {
    let mut p = platform();
    p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
    let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
    let blob = String::from_utf8_lossy(&dir);
    for name in [ACPI_RSDP_FILE, ACPI_TABLE_FILE, ACPI_LOADER_FILE] {
        assert!(blob.contains(name), "default fw_cfg dir missing {name}");
    }
}

#[test]
fn generated_smbios_tables_are_registered_by_default() {
    let mut p = platform();
    p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
    let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
    let blob = String::from_utf8_lossy(&dir);
    for name in [SMBIOS_ANCHOR_FILE, SMBIOS_TABLE_FILE] {
        assert!(blob.contains(name), "default fw_cfg dir missing {name}");
    }
}

#[test]
fn default_fw_cfg_matches_qemu_display_none_without_ramfb_file() {
    let mut p = platform();

    assert_eq!(find_fw_cfg_file_entry(&mut p, b"etc/ramfb"), None);
    assert_eq!(p.ramfb_config(), None);
}

#[test]
fn ramfb_opt_in_registers_qemu_ramfb_file() {
    let mut p = platform_with_ramfb();
    let (_, size) = fw_cfg_file_entry(&mut p, b"etc/ramfb");

    assert_eq!(size, RAMFB_CONFIG_SIZE);
    assert_eq!(p.ramfb_config(), None);
}

#[test]
fn platform_device_disable_omits_ramfb_fw_cfg_surface() {
    let mut p = platform_with_devices(VirtPlatformDeviceConfig {
        ramfb_present: false,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let ctrl = machine::RAM_BASE + 0x200;
    let config = [0u8; RAMFB_CONFIG_SIZE];

    assert_eq!(find_fw_cfg_file_entry(&mut p, b"etc/ramfb"), None);
    assert!(mem.write_bytes(ctrl, &config));
    assert_eq!(
        p.on_mmio(
            machine::FW_CFG.base + REG_DMA,
            MmioOp::Write {
                size: 8,
                value: ctrl.swap_bytes()
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(p.ramfb_config(), None);
}

#[test]
fn flat_guest_ram_rejects_ranges_that_overflow_host_offset() {
    let mut ram = FlatGuestRam::new(0, 16);
    let overflowing_gpa = usize::MAX as u64;

    assert_eq!(ram.read_bytes(overflowing_gpa, 2), None);
    assert!(!ram.write_bytes(overflowing_gpa, &[1, 2]));
}

#[test]
fn fw_cfg_dma_write_updates_ramfb_config() {
    let mut p = platform_with_ramfb();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let (selector, size) = fw_cfg_file_entry(&mut p, b"etc/ramfb");
    let src = machine::RAM_BASE + 0x100;
    let ctrl = machine::RAM_BASE + 0x200;
    let mut config = [0u8; RAMFB_CONFIG_SIZE];
    config[0..8].copy_from_slice(&0x4010_0000u64.to_be_bytes());
    config[8..12].copy_from_slice(&DRM_FORMAT_XRGB8888.to_be_bytes());
    config[12..16].copy_from_slice(&0u32.to_be_bytes());
    config[16..20].copy_from_slice(&1024u32.to_be_bytes());
    config[20..24].copy_from_slice(&768u32.to_be_bytes());
    config[24..28].copy_from_slice(&(1024u32 * 4).to_be_bytes());
    let control = (u32::from(selector) << 16) | DMA_CTL_SELECT | DMA_CTL_WRITE;
    let mut dma = Vec::new();
    dma.extend_from_slice(&control.to_be_bytes());
    dma.extend_from_slice(&(size as u32).to_be_bytes());
    dma.extend_from_slice(&src.to_be_bytes());
    assert!(mem.write_bytes(src, &config));
    assert!(mem.write_bytes(ctrl, &dma));

    let outcome = p.on_mmio(
        machine::FW_CFG.base + REG_DMA,
        MmioOp::Write {
            size: 8,
            value: ctrl.swap_bytes(),
        },
        &mut mem,
    );

    assert_eq!(outcome, MmioOutcome::WriteAck);
    assert_eq!(
        p.ramfb_config(),
        Some(RamfbConfig {
            addr: 0x4010_0000,
            fourcc: DRM_FORMAT_XRGB8888,
            flags: 0,
            width: 1024,
            height: 768,
            stride: 4096,
        })
    );
}

#[test]
fn default_fw_cfg_bootorder_targets_qemu_virtio_blk_pci_installer() {
    let mut p = platform();
    let bootorder = fw_cfg_file_entry(&mut p, b"bootorder");
    assert_eq!(bootorder.1, bootorder::QEMU_VIRTIO_BLK_PCI_BOOTORDER.len());

    p.fw_cfg.select(bootorder.0);
    assert_eq!(
        p.fw_cfg.read_data(bootorder.1),
        bootorder::QEMU_VIRTIO_BLK_PCI_BOOTORDER
    );
}

#[test]
fn acpi_and_smbios_tables_register_into_fw_cfg() {
    let mut p = platform();
    p.set_acpi_tables(vec![0xAA; 36], vec![0xBB; 100], vec![0xCC; 40]);
    p.set_smbios(vec![0x5F; 24], vec![0x01; 80]);
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    // Read the FILE_DIR through fw_cfg and confirm the names are present.
    p.fw_cfg.select(crate::fwcfg::KEY_FILE_DIR);
    let dir = p.fw_cfg.read_data(p.fw_cfg.file_dir_bytes().len());
    let blob = String::from_utf8_lossy(&dir);
    for name in [
        "etc/acpi/rsdp",
        "etc/acpi/tables",
        "etc/table-loader",
        "etc/smbios/smbios-anchor",
        "etc/smbios/smbios-tables",
        "bootorder",
    ] {
        assert!(blob.contains(name), "fw_cfg dir missing {name}");
    }
    // Suppress unused-variable warning for `mem` in this assertion-only test.
    let _ = &mut mem;
}

#[test]
fn report_pacing_zero_interval_is_unpaced() {
    let base = Instant::now();
    assert!(report_pacing_allows_emission(Duration::ZERO, None, base));
    assert!(report_pacing_allows_emission(
        Duration::ZERO,
        Some(base),
        base
    ));
    assert!(report_pacing_allows_emission(
        Duration::ZERO,
        Some(base + Duration::from_millis(1)),
        base
    ));
}

#[test]
fn report_pacing_first_emission_allowed_then_gated_until_interval_elapses() {
    let base = Instant::now();
    let interval = Duration::from_millis(30);
    // Nothing emitted yet: the first report is always allowed.
    assert!(report_pacing_allows_emission(interval, None, base));
    // Just emitted at `base`: held off until the full interval passes.
    assert!(!report_pacing_allows_emission(interval, Some(base), base));
    assert!(!report_pacing_allows_emission(
        interval,
        Some(base),
        base + Duration::from_millis(29)
    ));
    assert!(report_pacing_allows_emission(
        interval,
        Some(base),
        base + Duration::from_millis(30)
    ));
    assert!(report_pacing_allows_emission(
        interval,
        Some(base),
        base + Duration::from_millis(31)
    ));
}

#[test]
fn report_pacing_tolerates_now_before_last_emission() {
    // A non-monotonic clock (now earlier than the last emission) must not
    // underflow into "allowed"; saturating_duration_since yields zero.
    let base = Instant::now();
    let interval = Duration::from_millis(30);
    let last = base + Duration::from_millis(100);
    assert!(!report_pacing_allows_emission(interval, Some(last), base));
}

#[test]
fn three_d_scanout_readback_defaults_to_display_cadence() {
    assert_eq!(
        virtio_gpu_3d_scanout_readback_interval_from_value(None),
        Duration::from_millis(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS)
    );
    assert_eq!(
        virtio_gpu_3d_scanout_readback_interval_from_value(Some("invalid")),
        Duration::from_millis(DEFAULT_VIRTIO_GPU_3D_SCANOUT_READBACK_MS)
    );
}

#[test]
fn host_vblank_pacing_is_opt_in_with_a_120_hz_configured_default() {
    assert_eq!(virtio_gpu_vblank_interval_from_value(None), Duration::ZERO);
    assert_eq!(
        virtio_gpu_vblank_interval_from_value(Some("0")),
        Duration::ZERO
    );
    assert_eq!(
        virtio_gpu_vblank_interval_from_value(Some("120")),
        Duration::from_nanos(8_333_333)
    );
    assert_eq!(
        virtio_gpu_vblank_interval_from_value(Some("invalid")),
        Duration::from_nanos(8_333_333)
    );
}

#[test]
fn three_d_scanout_readback_allows_explicit_pacing_and_unpaced_debugging() {
    assert_eq!(
        virtio_gpu_3d_scanout_readback_interval_from_value(Some("33")),
        Duration::from_millis(33)
    );
    assert_eq!(
        virtio_gpu_3d_scanout_readback_interval_from_value(Some("0")),
        Duration::ZERO
    );
}

#[test]
fn opt_in_hda_pci_bar_routes_controller_mmio() {
    let mut p = platform_with_devices(VirtPlatformDeviceConfig {
        hda_present: true,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
    let bar = machine::PCIE_MMIO_32.base + 0x70_000;
    let cfg = |reg| pcie_cfg_gpa(crate::pcie::HDA_BDF.1, crate::pcie::HDA_BDF.2, reg);

    assert_eq!(
        p.on_mmio(
            cfg(crate::pcie::REG_VENDOR_DEVICE),
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0x2668_8086)
    );
    assert_eq!(
        p.on_mmio(
            cfg(crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: bar
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            cfg(crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            bar + crate::hda::REG_GCAP,
            MmioOp::Read { size: 2 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0x1001)
    );
    assert_eq!(
        p.on_mmio(
            bar + crate::hda::REG_GCTL,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            bar + crate::hda::REG_STATESTS,
            MmioOp::Read { size: 2 },
            &mut mem
        ),
        MmioOutcome::ReadValue(1)
    );
}

#[test]
fn poll_hda_routes_stream_ioc_through_standard_msi_aggregation() {
    let mut p = platform_with_devices(VirtPlatformDeviceConfig {
        hda_present: true,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
    let hda_bar = machine::PCIE_MMIO_32.base + 0x70_000;
    let cfg = |reg| pcie_cfg_gpa(crate::pcie::HDA_BDF.1, crate::pcie::HDA_BDF.2, reg);

    assert_eq!(
        p.on_mmio(
            cfg(crate::pcie::REG_BAR0),
            MmioOp::Write {
                size: 4,
                value: hda_bar,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            cfg(crate::pcie::REG_COMMAND_STATUS),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    let msi = u16::from(crate::pcie::HDA_MSI_CAP_OFFSET);
    let message_address = 0x0000_0001_0808_4000u64;
    assert_eq!(
        p.on_mmio(
            cfg(msi + 4),
            MmioOp::Write {
                size: 4,
                value: message_address as u32 as u64,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            cfg(msi + 8),
            MmioOp::Write {
                size: 4,
                value: message_address >> 32,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            cfg(msi + 12),
            MmioOp::Write {
                size: 2,
                value: 0x61,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            cfg(msi + 2),
            MmioOp::Write {
                size: 2,
                value: 0x0001,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    let bdl = machine::RAM_BASE + 0x1000;
    let pcm = machine::RAM_BASE + 0x2000;
    let pcm_bytes = vec![0x5a; 192];
    assert!(mem.write_bytes(pcm, &pcm_bytes));
    let mut descriptor = [0u8; 16];
    descriptor[..8].copy_from_slice(&pcm.to_le_bytes());
    descriptor[8..12].copy_from_slice(&(pcm_bytes.len() as u32).to_le_bytes());
    descriptor[12..16].copy_from_slice(&1u32.to_le_bytes());
    assert!(mem.write_bytes(bdl, &descriptor));

    let hda = p.hda.as_mut().expect("opt-in HDA controller");
    hda.mmio_write(crate::hda::REG_GCTL, 4, 1, &mut mem);
    hda.mmio_write(crate::hda::REG_SD_BDPL, 4, bdl, &mut mem);
    hda.mmio_write(crate::hda::REG_SD_CBL, 4, pcm_bytes.len() as u64, &mut mem);
    hda.mmio_write(crate::hda::REG_SD_LVI, 2, 0, &mut mem);
    hda.mmio_write(crate::hda::REG_SD_FMT, 2, 0x0011, &mut mem);
    hda.mmio_write(
        crate::hda::REG_INTCTL,
        4,
        u64::from((1u32 << 31) | 1),
        &mut mem,
    );
    hda.mmio_write(crate::hda::REG_SD_CTL, 1, 0x06, &mut mem);

    p.poll_hda(&mut mem);
    let mut messages = Vec::new();
    p.drain_pending_msix_into(&mut messages);
    assert_eq!(
        messages,
        vec![MsixMessage {
            vector: 0,
            address: message_address,
            data: 0x61,
        }]
    );
    assert!(
        p.take_pending_spi_levels().is_empty(),
        "enabled HDA MSI must not touch legacy PCI INTA"
    );
}
