//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
use crate::pcie::PcieMmioTarget;
use crate::pcie::PciePioTarget;
use crate::pcie::VIRTIO_BLK_BDF;
use crate::pcie::XHCI_BDF;
use crate::virtio_blk::INSTALLER_ISO_SLOT;
use crate::virtio_net::NetBackend;
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
    const REG_QUEUE_NUM: u64 = 0x038;
    const REG_QUEUE_READY: u64 = 0x044;
    const REG_QUEUE_NOTIFY: u64 = 0x050;
    const REG_QUEUE_DESC_LOW: u64 = 0x080;
    const REG_QUEUE_DRIVER_LOW: u64 = 0x090;
    const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;
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

    for (reg, value) in [
        (REG_QUEUE_NUM, 8),
        (REG_QUEUE_DESC_LOW, desc),
        (REG_QUEUE_DRIVER_LOW, avail),
        (REG_QUEUE_DEVICE_LOW, used),
        (REG_QUEUE_READY, 1),
    ] {
        assert_eq!(
            p.on_mmio(bar + reg, MmioOp::Write { size: 4, value }, &mut mem),
            MmioOutcome::WriteAck
        );
    }
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
fn pcie_virtio_net_opt_in_routes_tx_msix_and_reset_clears_runtime_state() {
    const MSI_ADDRESS: u64 = machine::GIC_ITS.base + 0x80;
    const MSI_DATA: u32 = 0x61;

    let mut p = platform_with_devices(VirtPlatformDeviceConfig {
        virtio_net_present: true,
        virtio_net_backend: VirtioNetBackendKind::Loopback,
        ..VirtPlatformDeviceConfig::default()
    });
    let mut mem = FlatGuestRam::new(machine::RAM_BASE, 0x20000);
    let bar4 = machine::PCIE_MMIO_32.base + 0x4_0000;
    let bar1 = machine::PCIE_MMIO_32.base + 0x5_0000;

    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                crate::pcie::REG_VENDOR_DEVICE,
            ),
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(0x1041_1af4)
    );
    program_virtio_net_bar4(&mut p, &mut mem, bar4);
    program_virtio_net_bar1(&mut p, &mut mem, bar1);
    assert_eq!(
        p.pcie_mmio_target(bar4),
        Some(PcieMmioTarget {
            bdf: crate::pcie::VIRTIO_NET_BDF,
            bar_index: 4,
            offset: 0,
        })
    );
    assert_eq!(
        p.pcie_mmio_target(bar1),
        Some(PcieMmioTarget {
            bdf: crate::pcie::VIRTIO_NET_BDF,
            bar_index: 1,
            offset: 0,
        })
    );

    let desc = machine::RAM_BASE + 0x10000;
    let avail = machine::RAM_BASE + 0x11000;
    let used = machine::RAM_BASE + 0x12000;
    let hdr = machine::RAM_BASE + 0x13000;
    let payload = machine::RAM_BASE + 0x14000;
    let frame = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00platform";

    setup_virtio_net_queue(
        &mut p,
        &mut mem,
        bar4,
        NET_TX_QUEUE,
        TestVirtQueue { desc, avail, used },
        NET_TX_QUEUE,
    );
    enable_virtio_net_msix_vector(&mut p, &mut mem, bar1, NET_TX_QUEUE, MSI_ADDRESS, MSI_DATA);
    assert!(mem.write_bytes(hdr, &[0; NET_VIRTIO_HDR_LEN]));
    assert!(mem.write_bytes(payload, frame));
    write_vring_desc(
        &mut mem,
        desc,
        0,
        hdr,
        NET_VIRTIO_HDR_LEN as u32,
        NET_DESC_F_NEXT,
        1,
    );
    write_vring_desc(&mut mem, desc, 1, payload, frame.len() as u32, 0, 0);
    assert!(mem.write_bytes(avail + 2, &1u16.to_le_bytes()));
    assert!(mem.write_bytes(avail + 4, &0u16.to_le_bytes()));

    assert_eq!(
        p.on_mmio(
            bar4 + NET_NOTIFY_CFG_OFFSET + u64::from(NET_TX_QUEUE) * 4,
            MmioOp::Write { size: 4, value: 0 },
            &mut mem,
        ),
        MmioOutcome::WriteAck
    );

    let net = p.virtio_net.as_ref().expect("virtio-net device present");
    assert_eq!(
        net.backend().test_transmitted_frames(),
        Some(&[frame.to_vec()][..])
    );
    assert_eq!(
        p.pending_msix,
        vec![crate::msix::MsixMessage {
            vector: NET_TX_QUEUE,
            address: MSI_ADDRESS,
            data: MSI_DATA,
        }]
    );
    let stats = p.virtio_net_stats().unwrap();
    assert_eq!(stats.tx_count, 1);
    assert_eq!(stats.queues[usize::from(NET_TX_QUEUE)].last_avail_idx, 1);

    p.reset();

    assert_eq!(
        p.take_pending_msix(),
        Vec::<crate::msix::MsixMessage>::new()
    );
    assert_eq!(p.pcie_mmio_target(bar4), None);
    let stats = p.virtio_net_stats().unwrap();
    assert_eq!(stats.tx_count, 0);
    assert_eq!(stats.notify_count, 0);
    assert!(!stats.queues[usize::from(NET_TX_QUEUE)].ready);
    assert_eq!(stats.status, 0);

    assert_eq!(
        p.on_mmio(
            pcie_cfg_gpa(
                crate::pcie::VIRTIO_NET_BDF.1,
                crate::pcie::VIRTIO_NET_BDF.2,
                crate::pcie::REG_VENDOR_DEVICE,
            ),
            MmioOp::Read { size: 4 },
            &mut mem,
        ),
        MmioOutcome::ReadValue(0x1041_1af4)
    );
    program_virtio_net_bar1(&mut p, &mut mem, bar1);
    assert_eq!(
        p.on_mmio(bar1 + 12, MmioOp::Read { size: 4 }, &mut mem),
        MmioOutcome::ReadValue(1)
    );
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
fn xhci_bar_reports_qemu_capability_registers() {
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
