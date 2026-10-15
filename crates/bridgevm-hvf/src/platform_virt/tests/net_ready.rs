//! The platform TX interrupt/reset fixture starts from a driver-ready device.
use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use crate::machine;
use crate::pcie::PcieMmioTarget;
use crate::virtio_net::NetBackend;

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
    virtio_net_write(&mut p, &mut mem, bar4, 0x14, 1, 0x0f);
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
