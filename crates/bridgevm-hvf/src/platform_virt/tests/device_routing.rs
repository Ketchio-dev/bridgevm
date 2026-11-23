use super::super::*;
use super::helpers::*;
use super::pci_boot_media_queue::ready_pci_boot_media_queue;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
use crate::pcie::PcieMmioTarget;
use crate::pcie::PciePioTarget;
use crate::pcie::VIRTIO_BLK_BDF;
use crate::pcie::XHCI_BDF;
use crate::ramfb::RAMFB_CONFIG_SIZE;
use crate::virtio_blk::INSTALLER_ISO_SLOT;
use std::fs;

#[test]
fn virtio_iso_completion_queues_legacy_spi_level_changes() {
    const REG_GUEST_PAGE_SIZE: u64 = 0x28;
    const REG_QUEUE_NUM: u64 = 0x38;
    const REG_QUEUE_ALIGN: u64 = 0x3c;
    const REG_QUEUE_PFN: u64 = 0x40;
    const REG_QUEUE_NOTIFY: u64 = 0x50;
    const REG_INTERRUPT_ACK: u64 = 0x64;
    const DESC_F_NEXT: u16 = 1;
    const DESC_F_WRITE: u16 = 2;
    const VIRTIO_BLK_T_IN: u32 = 0;
    const VIRTIO_BLK_S_OK: u8 = 0;

    let path = temp_path("virtio-iso");
    let mut media = vec![0u8; 1024];
    media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&path, media).unwrap();

    let mut p = platform();
    p.attach_virtio_iso(&path).unwrap();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
    let slot_base = machine::virtio_mmio_slot(INSTALLER_ISO_SLOT).base;
    let desc = machine::RAM_BASE + 0x1000;
    let avail = desc + 8 * 16;
    let used = (avail + 4 + 8 * 2).div_ceil(4096) * 4096;
    let header = machine::RAM_BASE + 0x4000;
    let data = machine::RAM_BASE + 0x5000;
    let status = machine::RAM_BASE + 0x6000;

    assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
    assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
    write_vring_desc(&mut mem, desc, 0, header, 16, DESC_F_NEXT, 1);
    write_vring_desc(&mut mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
    write_vring_desc(&mut mem, desc, 2, status, 1, DESC_F_WRITE, 0);
    assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
    assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

    for (reg, value) in [
        (REG_QUEUE_NUM, 8),
        (REG_GUEST_PAGE_SIZE, 4096),
        (REG_QUEUE_ALIGN, 4096),
        (REG_QUEUE_PFN, desc >> 12),
    ] {
        assert_eq!(
            p.on_mmio(slot_base + reg, MmioOp::Write { size: 4, value }, &mut mem),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(
        p.on_mmio(
            slot_base + REG_QUEUE_NOTIFY,
            MmioOp::Write { size: 4, value: 0 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );

    assert_eq!(mem.read_bytes(data, 8).unwrap(), b"WINSETUP");
    assert_eq!(mem.read_bytes(status, 1).unwrap(), [VIRTIO_BLK_S_OK]);
    assert_eq!(
        u16::from_le_bytes(mem.read_bytes(used + 2, 2).unwrap().try_into().unwrap()),
        1
    );
    assert_eq!(
        p.take_pending_spi_levels(),
        vec![(
            machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
            true
        )]
    );

    assert_eq!(
        p.on_mmio(
            slot_base + REG_INTERRUPT_ACK,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.take_pending_spi_levels(),
        vec![(
            machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
            false
        )]
    );

    fs::remove_file(path).ok();
}

#[test]
fn pcie_boot_media_reads_from_attached_iso_and_posts_interrupt() {
    const PCI_ISR_CFG_OFFSET: u64 = 0x1000;
    const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;
    const REG_QUEUE_NOTIFY: u64 = 0x050;
    const DESC_F_NEXT: u16 = 1;
    const DESC_F_WRITE: u16 = 2;
    const VIRTIO_BLK_T_IN: u32 = 0;
    const VIRTIO_BLK_S_OK: u8 = 0;

    let path = temp_path("pci-boot-media");
    let mut media = vec![0u8; 1024];
    media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&path, media).unwrap();

    let mut p = platform();
    p.attach_pci_boot_media(&path).unwrap();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x10000);
    let bar = machine::PCIE_MMIO_32.base + 0x8000;
    program_virtio_blk_bar4(&mut p, &mut mem, bar);

    let desc = machine::RAM_BASE + 0x1000;
    let avail = machine::RAM_BASE + 0x2000;
    let used = machine::RAM_BASE + 0x3000;
    let header = machine::RAM_BASE + 0x4000;
    let data = machine::RAM_BASE + 0x5000;
    let status = machine::RAM_BASE + 0x6000;

    assert!(mem.write_bytes(header, &VIRTIO_BLK_T_IN.to_le_bytes()));
    assert!(mem.write_bytes(header + 8, &1u64.to_le_bytes()));
    write_vring_desc(&mut mem, desc, 0, header, 16, DESC_F_NEXT, 1);
    write_vring_desc(&mut mem, desc, 1, data, 512, DESC_F_NEXT | DESC_F_WRITE, 2);
    write_vring_desc(&mut mem, desc, 2, status, 1, DESC_F_WRITE, 0);
    assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
    assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

    ready_pci_boot_media_queue(&mut p, &mut mem, bar, desc, avail, used);
    assert_eq!(
        p.on_mmio(
            bar + PCI_NOTIFY_CFG_OFFSET + REG_QUEUE_NOTIFY,
            MmioOp::Write { size: 4, value: 0 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );

    assert_eq!(mem.read_bytes(data, 8).unwrap(), b"WINSETUP");
    assert_eq!(mem.read_bytes(status, 1).unwrap(), [VIRTIO_BLK_S_OK]);
    assert_eq!(
        p.take_pending_spi_levels(),
        vec![(machine::spi_to_intid(machine::SPI_PCIE_INTA), true)]
    );

    assert_eq!(
        p.on_mmio(
            bar + PCI_ISR_CFG_OFFSET,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.take_pending_spi_levels(),
        vec![(machine::spi_to_intid(machine::SPI_PCIE_INTA), false)]
    );

    fs::remove_file(path).ok();
}

#[test]
fn pcie_boot_media_msix_bar_decodes_table_and_pba() {
    let path = temp_path("pci-boot-media-msix");
    fs::write(&path, [0u8; 512]).unwrap();

    let mut p = platform();
    p.attach_pci_boot_media(&path).unwrap();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let bar = machine::PCIE_MMIO_32.base + 0x1_8000;
    program_virtio_blk_bar1(&mut p, &mut mem, bar);

    assert_eq!(
        p.pcie_mmio_target(bar),
        Some(PcieMmioTarget {
            bdf: VIRTIO_BLK_BDF,
            bar_index: 1,
            offset: 0,
        })
    );
    assert_eq!(
        p.on_mmio(bar + 12, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(1)
    );
    assert_eq!(
        p.on_mmio(
            bar,
            MmioOp::Write {
                size: 4,
                value: 0xfee0_0000,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(bar, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0xfee0_0000)
    );
    assert_eq!(
        p.on_mmio(
            bar + u64::from(crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET),
            MmioOp::Read { size: 8 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0)
    );
    assert_eq!(
        p.on_mmio(
            bar + u64::from(crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET),
            MmioOp::Write {
                size: 8,
                value: u64::MAX,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            bar + u64::from(crate::pcie::VIRTIO_BLK_MSIX_PBA_OFFSET),
            MmioOp::Read { size: 8 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0)
    );

    fs::remove_file(path).ok();
}

#[test]
fn pcie_boot_media_modern_bar_live_offsets_stay_modelled() {
    let path = temp_path("pci-boot-media-modern-offsets");
    fs::write(&path, [0u8; 512]).unwrap();

    let mut p = platform();
    p.attach_pci_boot_media(&path).unwrap();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    let bar = machine::PCIE_MMIO_32.base + 0x1_c000;
    program_virtio_blk_bar4(&mut p, &mut mem, bar);

    for offset in [0x20, 0x28, 0x30, 0x38, 0x40, 0x48, 0x50, 0x58] {
        assert!(matches!(
            p.on_mmio(bar + offset, MmioOp::Read { size: 4 }, &mut mem),
            MmioOutcome::ReadValue(_)
        ));
        assert_eq!(
            p.on_mmio(bar + offset, MmioOp::Write { size: 4, value: 0 }, &mut mem),
            MmioOutcome::WriteAck
        );
    }

    fs::remove_file(path).ok();
}

#[test]
fn pcie_boot_media_legacy_pio_bar_decodes_without_unimplemented() {
    let path = temp_path("pci-boot-media-pio");
    fs::write(&path, [0u8; 512]).unwrap();

    let mut p = platform();
    p.attach_pci_boot_media(&path).unwrap();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x1000);
    program_virtio_blk_bar0_pio(&mut p, &mut mem, 0);

    assert_eq!(
        p.pcie_pio_target(machine::PCIE_PIO.base),
        Some(PciePioTarget {
            bdf: VIRTIO_BLK_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
    assert_eq!(
        p.on_mmio(machine::PCIE_PIO.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x60)
    );
    assert_eq!(
        p.on_mmio(
            machine::PCIE_PIO.base + 0x12,
            MmioOp::Write { size: 1, value: 4 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(p.pci_boot_media_stats().unwrap().status, 4);

    fs::remove_file(path).ok();
}

#[test]
fn unknown_pcie_bar_still_reports_known_unimplemented() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);

    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + 0x1_0000,
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::KnownUnimplemented("pcie-mmio-32")
    );
}

#[test]
fn xhci_bar_and_command_do_not_enable_nvme_liveness_or_decode() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let xhci_base = machine::PCIE_MMIO_32.base + 0x2_0000;

    // Given: only xHCI BAR0 and command bits are programmed.
    for (reg, value) in [
        (crate::pcie::REG_BAR0, xhci_base),
        (crate::pcie::REG_BAR0 + 4, 0),
        (
            crate::pcie::REG_COMMAND_STATUS,
            u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
        ),
    ] {
        assert_eq!(
            p.on_mmio(
                pcie_cfg_gpa(crate::pcie::XHCI_BDF.1, crate::pcie::XHCI_BDF.2, reg),
                MmioOp::Write {
                    size: if reg == crate::pcie::REG_COMMAND_STATUS {
                        2
                    } else {
                        4
                    },
                    value,
                },
                &mut mem,
            ),
            MmioOutcome::WriteAck
        );
    }

    // Then: xHCI decode exists, but NVMe liveness and NVMe BAR decode do not.
    let live = p.nvme_pcie_liveness();
    assert!(live.nvme_advertised);
    assert!(!live.nvme_ecam_touched);
    assert!(!live.nvme_bar0_assigned);
    assert!(!live.nvme_command_memory_enabled);
    assert!(!live.nvme_command_bus_master_enabled);
    assert_eq!(p.pcie_mmio_target(machine::PCIE_MMIO_32.base), None);
    assert_eq!(
        p.pcie_mmio_target(xhci_base),
        Some(PcieMmioTarget {
            bdf: XHCI_BDF,
            bar_index: 0,
            offset: 0,
        })
    );
}

#[test]
fn xhci_bar_reports_declared_capability_registers() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let base = machine::PCIE_MMIO_32.base + 0x2_0000;

    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::XHCI_BDF.1,
                crate::pcie::XHCI_BDF.2,
                crate::pcie::REG_BAR0
            ),
            MmioOp::Write {
                size: 4,
                value: base,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::XHCI_BDF.1,
                crate::pcie::XHCI_BDF.2,
                crate::pcie::REG_BAR0 + 4
            ),
            MmioOp::Write { size: 4, value: 0 },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::XHCI_BDF.1,
                crate::pcie::XHCI_BDF.2,
                crate::pcie::REG_COMMAND_STATUS
            ),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(base, MmioOp::Read { size: 1 }, &mut mem),
        MmioOutcome::ReadValue(0x40)
    );
    assert_eq!(
        p.on_mmio(base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0100_0040)
    );
    assert_eq!(
        p.on_mmio(base + 0x04, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0800_1040)
    );
    assert_eq!(
        p.on_mmio(base + 0x08, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0000_000f)
    );
    assert_eq!(
        p.on_mmio(base + 0x10, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0008_7001)
    );
    assert_eq!(
        p.on_mmio(base + 0x14, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0000_2000)
    );
    assert_eq!(
        p.on_mmio(base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0000_1000)
    );
}

#[test]
fn xhci_bar_routes_from_64bit_mmio_window() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    let base = machine::PCIE_MMIO_64.base + 0x2_0000;

    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::XHCI_BDF.1,
                crate::pcie::XHCI_BDF.2,
                crate::pcie::REG_BAR0
            ),
            MmioOp::Write {
                size: 4,
                value: base & 0xffff_ffff,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::XHCI_BDF.1,
                crate::pcie::XHCI_BDF.2,
                crate::pcie::REG_BAR0 + 4
            ),
            MmioOp::Write {
                size: 4,
                value: base >> 32,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::XHCI_BDF.1,
                crate::pcie::XHCI_BDF.2,
                crate::pcie::REG_COMMAND_STATUS
            ),
            MmioOp::Write {
                size: 2,
                value: u64::from(crate::pcie::CMD_MEMORY_SPACE | crate::pcie::CMD_BUS_MASTER),
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );

    assert_eq!(
        p.on_mmio(base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0100_0040)
    );
    assert_eq!(
        p.on_mmio(base + 0x04, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0800_1040)
    );
}

#[test]
fn pcie_nvme_reads_and_writes_preloaded_disk_media() {
    const ASQ: u64 = machine::RAM_BASE + 0x1000;
    const ACQ: u64 = machine::RAM_BASE + 0x2000;
    const IO_SQ: u64 = machine::RAM_BASE + 0x3000;
    const IO_CQ: u64 = machine::RAM_BASE + 0x4000;
    const DATA: u64 = machine::RAM_BASE + 0x5000;
    const SLBA: u64 = 7;

    let mut p = platform();
    let mut disk = vec![0u8; crate::nvme::LBA_SIZE * 16];
    let pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
        .map(|i| 0x80 | ((i % 0x40) as u8))
        .collect();
    let start = SLBA as usize * crate::nvme::LBA_SIZE;
    disk[start..start + crate::nvme::LBA_SIZE].copy_from_slice(&pattern);
    p.load_nvme_disk(disk);

    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x9000);
    program_nvme_bar0(&mut p, &mut mem);
    enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

    let cdw10 = (3u32 << 16) | 1;
    let create_cq = encode_nvme_sqe(0x05, 1, 0, IO_CQ, cdw10, 1, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
    let create_sq = encode_nvme_sqe(0x01, 2, 0, IO_SQ, cdw10, (1u32 << 16) | 1, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);

    let read = encode_nvme_sqe(
        0x02,
        0x10,
        crate::nvme::NSID,
        DATA,
        SLBA as u32,
        (SLBA >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ, &read));
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        mem.read_bytes(DATA, crate::nvme::LBA_SIZE).unwrap(),
        pattern
    );

    let replacement: Vec<u8> = (0..crate::nvme::LBA_SIZE)
        .map(|i| 0x40 | ((i % 0x20) as u8))
        .collect();
    assert!(mem.write_bytes(DATA, &replacement));
    let write = encode_nvme_sqe(
        0x01,
        0x11,
        crate::nvme::NSID,
        DATA,
        SLBA as u32,
        (SLBA >> 32) as u32,
        0,
    );
    assert!(mem.write_bytes(IO_SQ + crate::nvme::SQ_ENTRY_SIZE, &write));
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
            MmioOp::Write { size: 4, value: 2 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        &p.nvme_disk()[start..start + crate::nvme::LBA_SIZE],
        replacement.as_slice()
    );
}

#[test]
fn platform_reset_preserving_media_and_vars_clears_runtime_state() {
    const ASQ: u64 = machine::RAM_BASE + 0x1000;
    const ACQ: u64 = machine::RAM_BASE + 0x2000;
    const IO_SQ: u64 = machine::RAM_BASE + 0x3000;
    const IO_CQ: u64 = machine::RAM_BASE + 0x4000;
    const DATA: u64 = machine::RAM_BASE + 0x5000;
    const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x40;
    const MSI_DATA: u32 = 35;

    // Given: persistent media, virtio installer media, RAMFB fw_cfg bytes,
    // and UEFI vars have guest-visible writes, while device runtime state
    // and pending interrupts are dirty.
    let virtio_iso_path = temp_path("reset-virtio-iso");
    let pci_boot_media_path = temp_path("reset-pci-boot-media");
    let mut installer_media = vec![0u8; 1024];
    installer_media[512..520].copy_from_slice(b"WINSETUP");
    fs::write(&virtio_iso_path, &installer_media).unwrap();
    fs::write(&pci_boot_media_path, &installer_media).unwrap();

    let mut p = platform_with_ramfb();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x18000);
    p.attach_virtio_iso(&virtio_iso_path).unwrap();
    p.attach_pci_boot_media(&pci_boot_media_path).unwrap();
    write_valid_ramfb_config(&mut p, &mut mem);
    assert!(p.ramfb_config().is_some());
    p.load_nvme_disk(vec![0u8; crate::nvme::LBA_SIZE * 16]);
    p.attach_nvme_second_namespace(crate::nvme::LBA_SIZE * 8);
    program_nvme_bar0(&mut p, &mut mem);
    enable_nvme_msix_vector0(&mut p, &mut mem, MSI_ADDRESS, MSI_DATA);
    enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);

    let cdw10 = (3u32 << 16) | 1;
    let create_cq = encode_nvme_sqe(0x05, 1, 0, IO_CQ, cdw10, 1, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
    let create_sq = encode_nvme_sqe(0x01, 2, 0, IO_SQ, cdw10, (1u32 << 16) | 1, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);

    let ns1_pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
        .map(|i| 0x20 | ((i % 0x20) as u8))
        .collect();
    assert!(mem.write_bytes(DATA, &ns1_pattern));
    let ns1_write = encode_nvme_sqe(0x01, 0x31, crate::nvme::NSID, DATA, 2, 0, 0);
    assert!(mem.write_bytes(IO_SQ, &ns1_write));
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    let ns2_pattern: Vec<u8> = (0..crate::nvme::LBA_SIZE)
        .map(|i| 0x80 | ((i % 0x40) as u8))
        .collect();
    assert!(mem.write_bytes(DATA, &ns2_pattern));
    let ns2_write = encode_nvme_sqe(0x01, 0x32, crate::nvme::NSID2, DATA, 0, 0, 0);
    assert!(mem.write_bytes(IO_SQ + crate::nvme::SQ_ENTRY_SIZE, &ns2_write));
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
            MmioOp::Write { size: 4, value: 2 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    let identify_controller = encode_nvme_sqe(0x06, 0x33, 0, DATA, 0x01, 0, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 2, &identify_controller);
    assert!(!p.pending_msix.is_empty());
    assert!(p.nvme_pcie_liveness().nvme_admin_doorbell_rung);
    assert!(!p.nvme_command_trace().is_empty());

    assert_eq!(read_virtio_iso_sector(&mut p, &mut mem, 1, 8), b"WINSETUP");
    let pci_boot_media_bar = machine::PCIE_MMIO_32.base + 0x8000;
    let pci_boot_media_msix_bar = machine::PCIE_MMIO_32.base + 0x1_8000;
    program_virtio_blk_bar4(&mut p, &mut mem, pci_boot_media_bar);
    assert_eq!(
        read_pci_boot_media_sector(&mut p, &mut mem, pci_boot_media_bar, 1, 8),
        b"WINSETUP"
    );
    program_virtio_blk_bar1(&mut p, &mut mem, pci_boot_media_msix_bar);
    assert_eq!(
        p.on_mmio(
            pci_boot_media_msix_bar,
            MmioOp::Write {
                size: 4,
                value: 0xfee0_0000,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            pci_boot_media_msix_bar + 8,
            MmioOp::Write {
                size: 4,
                value: 0x45,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(pci_boot_media_msix_bar, MmioOp::Read { size: 4 }, &mut mem,),
        MmioOutcome::ReadValue(0xfee0_0000)
    );
    assert_eq!(
        p.on_mmio(
            pci_boot_media_msix_bar + 8,
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(0x45)
    );
    assert!(!p.virtio_iso_request_trace().unwrap().is_empty());
    assert!(!p.pci_boot_media_request_trace().unwrap().is_empty());

    p.load_flash_vars(&[0xff; 8]);
    assert_eq!(
        p.on_mmio(
            machine::FLASH_VARS.base,
            MmioOp::Write {
                size: 4,
                value: 0x0040_0040,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            machine::FLASH_VARS.base,
            MmioOp::Write {
                size: 4,
                value: 0x1234_5678,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    let flash_after_program = p.flash_vars_image()[0..8].to_vec();

    // When: the probe reboot loop asks the platform to reset runtime state.
    p.reset().unwrap();

    // Then: persistent media and vars survive, while PCIe/NVMe runtime state
    // no longer carries over to the next boot.
    let ns1_start = 2 * crate::nvme::LBA_SIZE;
    assert_eq!(
        &p.nvme_disk()[ns1_start..ns1_start + crate::nvme::LBA_SIZE],
        ns1_pattern.as_slice()
    );
    assert_eq!(&p.flash_vars_image()[0..8], flash_after_program.as_slice());
    assert_eq!(
        p.take_pending_msix(),
        Vec::<crate::msix::MsixMessage>::new()
    );
    assert_eq!(
        p.take_pending_spi_levels(),
        vec![
            (
                machine::spi_to_intid(machine::virtio_mmio_spi(INSTALLER_ISO_SLOT as u32)),
                false,
            ),
            (machine::spi_to_intid(machine::SPI_PCIE_INTA), false),
        ]
    );
    assert_eq!(p.take_pending_spi_levels(), Vec::<(u32, bool)>::new());
    assert_eq!(p.fw_cfg.read_data(4), b"QEMU");
    let (_, ramfb_size_after_reset) = fw_cfg_file_entry(&mut p, b"etc/ramfb");
    assert_eq!(ramfb_size_after_reset, RAMFB_CONFIG_SIZE);
    p.refresh_ramfb();
    assert_eq!(p.ramfb_config(), None);
    let virtio_iso_stats = p.virtio_iso_stats().unwrap();
    assert_eq!(virtio_iso_stats.request_count, 0);
    assert_eq!(virtio_iso_stats.read_count, 0);
    assert_eq!(virtio_iso_stats.notify_count, 0);
    assert!(!virtio_iso_stats.queue_ready);
    assert_eq!(virtio_iso_stats.status, 0);
    assert!(p.virtio_iso_request_trace().unwrap().is_empty());
    let pci_boot_media_stats = p.pci_boot_media_stats().unwrap();
    assert_eq!(pci_boot_media_stats.request_count, 0);
    assert_eq!(pci_boot_media_stats.read_count, 0);
    assert_eq!(pci_boot_media_stats.notify_count, 0);
    assert!(!pci_boot_media_stats.queue_ready);
    assert_eq!(pci_boot_media_stats.status, 0);
    assert!(p.pci_boot_media_request_trace().unwrap().is_empty());
    program_virtio_blk_bar1(&mut p, &mut mem, pci_boot_media_msix_bar);
    assert_eq!(
        p.on_mmio(pci_boot_media_msix_bar, MmioOp::Read { size: 4 }, &mut mem,),
        MmioOutcome::ReadValue(0)
    );
    assert_eq!(
        p.on_mmio(
            pci_boot_media_msix_bar + 8,
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(0)
    );
    assert_eq!(
        p.on_mmio(
            pci_boot_media_msix_bar + 12,
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(1)
    );
    let reset_liveness = p.nvme_pcie_liveness();
    assert!(reset_liveness.nvme_advertised);
    assert!(!reset_liveness.nvme_ecam_touched);
    assert!(!reset_liveness.nvme_command_memory_enabled);
    assert!(!reset_liveness.nvme_command_bus_master_enabled);
    assert!(!reset_liveness.nvme_bar0_assigned);
    assert!(!reset_liveness.nvme_mmio_reached);
    assert!(!reset_liveness.nvme_cc_enabled);
    assert!(!reset_liveness.nvme_admin_doorbell_rung);
    assert_eq!(p.pcie_mmio_target(machine::PCIE_MMIO_32.base), None);
    assert!(p.nvme_command_trace().is_empty());

    program_nvme_bar0(&mut p, &mut mem);
    enable_nvme_controller(&mut p, &mut mem, ASQ, ACQ);
    let cdw10 = (3u32 << 16) | 1;
    let create_cq = encode_nvme_sqe(0x05, 3, 0, IO_CQ, cdw10, 1, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 0, &create_cq);
    let create_sq = encode_nvme_sqe(0x01, 4, 0, IO_SQ, cdw10, (1u32 << 16) | 1, 0);
    submit_admin_sqe(&mut p, &mut mem, ASQ, 1, &create_sq);
    assert!(mem.write_bytes(DATA, &[0u8; crate::nvme::LBA_SIZE]));
    let ns2_read = encode_nvme_sqe(0x02, 0x34, crate::nvme::NSID2, DATA, 0, 0, 0);
    assert!(mem.write_bytes(IO_SQ, &ns2_read));
    assert_eq!(
        p.on_mmio(
            machine::PCIE_MMIO_32.base + crate::nvme::REG_DOORBELL_BASE + 2 * 4,
            MmioOp::Write { size: 4, value: 1 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        mem.read_bytes(DATA, crate::nvme::LBA_SIZE).unwrap(),
        ns2_pattern
    );
    assert_eq!(read_virtio_iso_sector(&mut p, &mut mem, 1, 8), b"WINSETUP");
    let pci_boot_media_bar = machine::PCIE_MMIO_32.base + 0x8000;
    program_virtio_blk_bar4(&mut p, &mut mem, pci_boot_media_bar);
    assert_eq!(
        read_pci_boot_media_sector(&mut p, &mut mem, pci_boot_media_bar, 1, 8),
        b"WINSETUP"
    );

    fs::remove_file(virtio_iso_path).ok();
    fs::remove_file(pci_boot_media_path).ok();
}

#[test]
fn uart_writes_are_captured_via_mmio() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    for b in b"HI\n" {
        assert_eq!(
            p.on_mmio(
                machine::UART.base,
                MmioOp::Write {
                    size: 1,
                    value: u64::from(*b)
                },
                &mut mem
            ),
            MmioOutcome::WriteAck
        );
    }
    assert_eq!(p.uart_output(), b"HI\n");
    // UARTFR (offset 0x18) reports idle FIFOs: TXFE and RXFE set.
    assert!(matches!(
        p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(v) if v & ((1 << 7) | (1 << 4)) == ((1 << 7) | (1 << 4))
    ));
}

#[test]
fn uart_reads_consume_preloaded_input_via_mmio() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.push_uart_input(b" ");
    assert_eq!(p.uart_input_len(), 1);
    assert!(matches!(
        p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(v) if v & (1 << 4) == 0
    ));
    assert_eq!(
        p.on_mmio(machine::UART.base, MmioOp::Read { size: 1 }, &mut mem),
        MmioOutcome::ReadValue(u64::from(b' '))
    );
    assert_eq!(p.uart_input_len(), 0);
    assert!(matches!(
        p.on_mmio(machine::UART.base + 0x18, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(v) if v & (1 << 4) != 0
    ));
}

#[test]
fn rtc_data_and_id_registers_are_modelled() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    assert_eq!(
        p.on_mmio(
            machine::RTC.base + 0xfe0,
            MmioOp::Read { size: 4 },
            &mut mem
        ),
        MmioOutcome::ReadValue(0x31)
    );
    match p.on_mmio(machine::RTC.base, MmioOp::Read { size: 4 }, &mut mem) {
        MmioOutcome::ReadValue(value) => assert!(value > 1_600_000_000),
        other => panic!("unexpected RTC read outcome: {other:?}"),
    }
    assert_eq!(
        p.on_mmio(
            machine::RTC.base + 0x008,
            MmioOp::Write {
                size: 4,
                value: 0x2026_0619,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(machine::RTC.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x2026_0619)
    );
}

#[test]
fn flash_vars_routes_nor_status_protocol() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.load_flash_vars(&[0x78, 0x56, 0x34, 0x12]);
    assert_eq!(
        p.on_mmio(machine::FLASH_VARS.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x1234_5678)
    );
    assert_eq!(
        p.on_mmio(
            machine::FLASH_VARS.base,
            MmioOp::Write {
                size: 4,
                value: 0x0070_0070,
            },
            &mut mem
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(machine::FLASH_VARS.base, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(0x0080_0080)
    );
}

#[test]
fn flash_vars_snapshot_reflects_guest_programming() {
    let mut p = platform();
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0);
    p.load_flash_vars(&[0xff; 8]);
    assert_eq!(p.flash_vars_image()[0], 0xff);

    assert_eq!(
        p.on_mmio(
            machine::FLASH_VARS.base,
            MmioOp::Write {
                size: 4,
                value: 0x0040_0040,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(
        p.on_mmio(
            machine::FLASH_VARS.base,
            MmioOp::Write {
                size: 4,
                value: 0x1234_5678,
            },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );
    assert_eq!(&p.flash_vars_image()[0..4], &[0x78, 0x56, 0x34, 0x12]);
}
