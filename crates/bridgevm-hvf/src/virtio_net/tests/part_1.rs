//! Split test module.

use super::super::*;
use crate::msix::MsixMessage;

use super::helpers::*;

#[test]
fn feature_negotiation_reads_both_windows_and_status_round_trips() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x1000);

    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)
    );
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_F_VERSION_1)
    );

    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
    pci_write(
        &mut dev,
        COMMON_DRIVER_FEATURE,
        4,
        u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS),
        &mut mem,
    );
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
    pci_write(
        &mut dev,
        COMMON_DRIVER_FEATURE,
        4,
        u64::from(VIRTIO_F_VERSION_1),
        &mut mem,
    );
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x0f, &mut mem);

    assert_eq!(pci_read(&mut dev, COMMON_DEVICE_STATUS, 1, &mut mem), 0x0f);
    assert_eq!(
        dev.stats().driver_features,
        u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS) | (u64::from(VIRTIO_F_VERSION_1) << 32)
    );
}

#[test]
fn modern_common_config_masks_features_and_accepts_split_queue_address_writes() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x1000);
    let rx_desc = 0x0000_0001_4000_1000;
    let rx_avail = 0x0000_0001_4000_2000;
    let rx_used = 0x0000_0001_4000_3000;
    let tx_desc = 0x0000_0001_4000_5000;
    let tx_avail = 0x0000_0001_4000_6000;
    let tx_used = 0x0000_0001_4000_7000;
    let offered =
        u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS) | (u64::from(VIRTIO_F_VERSION_1) << 32);

    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        0x0001_0020
    );
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        0x0000_0001
    );

    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0x112f_8001, &mut mem);
    assert_eq!(dev.stats().driver_features, offered);
    assert_eq!(dev.stats().driver_features & !offered, 0);

    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x01, &mut mem);
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x03, &mut mem);
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x07, &mut mem);

    for (queue, desc, avail, used, vector) in [
        (0, rx_desc, rx_avail, rx_used, 1u16),
        (1, tx_desc, tx_avail, tx_used, 0u16),
    ] {
        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, queue, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem),
            u64::from(QUEUE_MAX)
        );
        pci_write(
            &mut dev,
            COMMON_QUEUE_SIZE,
            2,
            u64::from(QUEUE_MAX),
            &mut mem,
        );
        pci_write_split_u64(&mut dev, COMMON_QUEUE_DESC, desc, &mut mem);
        pci_write_split_u64(&mut dev, COMMON_QUEUE_DRIVER, avail, &mut mem);
        pci_write_split_u64(&mut dev, COMMON_QUEUE_DEVICE, used, &mut mem);
        pci_write(
            &mut dev,
            COMMON_QUEUE_MSIX_VECTOR,
            2,
            u64::from(vector),
            &mut mem,
        );
        pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 1, &mut mem);
    }

    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x0f, &mut mem);

    let stats = dev.stats();
    assert_eq!(stats.status, 0x0f);
    assert_eq!(stats.queues[0].size, QUEUE_MAX);
    assert!(stats.queues[0].ready);
    assert_eq!(stats.queues[0].desc, rx_desc);
    assert_eq!(stats.queues[0].driver, rx_avail);
    assert_eq!(stats.queues[0].device, rx_used);
    assert_eq!(stats.queues[0].msix_vector, 1);
    assert_eq!(stats.queues[1].size, QUEUE_MAX);
    assert!(stats.queues[1].ready);
    assert_eq!(stats.queues[1].desc, tx_desc);
    assert_eq!(stats.queues[1].driver, tx_avail);
    assert_eq!(stats.queues[1].device, tx_used);
    assert_eq!(stats.queues[1].msix_vector, 0);
}

#[test]
fn modern_driver_common_config_sequence_advertises_and_enables_both_queues() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let rx_desc = 0x4000_1000;
    let rx_avail = 0x4000_2000;
    let rx_used = 0x4000_3000;
    let tx_desc = 0x4000_5000;
    let tx_avail = 0x4000_6000;
    let tx_used = 0x4000_7000;
    let tx_hdr = 0x4000_8000;
    let tx_payload = 0x4000_9000;
    let frame = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00modern";

    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x01, &mut mem);
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x03, &mut mem);

    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS)
    );
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_F_VERSION_1)
    );
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
    pci_write(
        &mut dev,
        COMMON_DRIVER_FEATURE,
        4,
        u64::from(VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS),
        &mut mem,
    );
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
    pci_write(
        &mut dev,
        COMMON_DRIVER_FEATURE,
        4,
        u64::from(VIRTIO_F_VERSION_1),
        &mut mem,
    );
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x07, &mut mem);
    assert_eq!(pci_read(&mut dev, COMMON_DEVICE_STATUS, 1, &mut mem), 0x07);
    assert_eq!(pci_read(&mut dev, COMMON_NUM_QUEUES, 2, &mut mem), 2);

    for (queue, desc, avail, used, vector) in [
        (0, rx_desc, rx_avail, rx_used, 0u16),
        (1, tx_desc, tx_avail, tx_used, 1u16),
    ] {
        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, queue, &mut mem);
        assert_eq!(
            pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem),
            u64::from(QUEUE_MAX)
        );
        pci_write(&mut dev, COMMON_QUEUE_SIZE, 2, 8, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_DESC, 8, desc, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_DRIVER, 8, avail, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_DEVICE, 8, used, &mut mem);
        pci_write(
            &mut dev,
            COMMON_QUEUE_MSIX_VECTOR,
            2,
            u64::from(vector),
            &mut mem,
        );
        pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 1, &mut mem);
    }

    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0x0f, &mut mem);

    let stats = dev.stats();
    assert_eq!(stats.status, 0x0f);
    assert_eq!(stats.queues[0].size, 8);
    assert!(stats.queues[0].ready);
    assert_eq!(stats.queues[0].desc, rx_desc);
    assert_eq!(stats.queues[0].driver, rx_avail);
    assert_eq!(stats.queues[0].device, rx_used);
    assert_eq!(stats.queues[0].msix_vector, 0);
    assert_eq!(stats.queues[1].size, 8);
    assert!(stats.queues[1].ready);
    assert_eq!(stats.queues[1].desc, tx_desc);
    assert_eq!(stats.queues[1].driver, tx_avail);
    assert_eq!(stats.queues[1].device, tx_used);
    assert_eq!(stats.queues[1].msix_vector, 1);

    mem.write(tx_hdr, &[0; VIRTIO_NET_HDR_LEN]);
    mem.write(tx_payload, frame);
    write_desc(
        &mut mem,
        tx_desc,
        0,
        tx_hdr,
        VIRTIO_NET_HDR_LEN as u32,
        DESC_F_NEXT,
        1,
    );
    write_desc(&mut mem, tx_desc, 1, tx_payload, frame.len() as u32, 0, 0);
    mem.write(tx_avail + 2, &1u16.to_le_bytes());
    mem.write(tx_avail + 4, &0u16.to_le_bytes());

    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

    assert_eq!(dev.backend().transmitted_frames(), &[frame.to_vec()]);
    assert_eq!(dev.stats().notify_count, 1);
}

#[test]
fn queue_setup_preserves_rx_and_tx_state_across_queue_selection() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    setup_queue(
        &mut dev,
        &mut mem,
        0,
        0x4000_1000,
        0x4000_2000,
        0x4000_3000,
        0,
    );
    setup_queue(
        &mut dev,
        &mut mem,
        1,
        0x4000_4000,
        0x4000_5000,
        0x4000_6000,
        1,
    );

    let stats = dev.stats();
    assert_eq!(stats.queues[0].size, 8);
    assert!(stats.queues[0].ready);
    assert_eq!(stats.queues[0].desc, 0x4000_1000);
    assert_eq!(stats.queues[0].driver, 0x4000_2000);
    assert_eq!(stats.queues[0].device, 0x4000_3000);
    assert_eq!(stats.queues[0].msix_vector, 0);
    assert_eq!(stats.queues[0].notify_off, 0);
    assert_eq!(stats.queues[1].size, 8);
    assert!(stats.queues[1].ready);
    assert_eq!(stats.queues[1].desc, 0x4000_4000);
    assert_eq!(stats.queues[1].driver, 0x4000_5000);
    assert_eq!(stats.queues[1].device, 0x4000_6000);
    assert_eq!(stats.queues[1].msix_vector, 1);
    assert_eq!(stats.queues[1].notify_off, 1);

    pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, 0, &mut mem);
    assert_eq!(pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem), 8);
    assert_eq!(pci_read(&mut dev, COMMON_QUEUE_NOTIFY_OFF, 2, &mut mem), 0);
    pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, 1, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_QUEUE_DESC, 4, &mut mem),
        0x4000_4000
    );
    assert_eq!(pci_read(&mut dev, COMMON_QUEUE_NOTIFY_OFF, 2, &mut mem), 1);
}

#[test]
fn tx_notify_strips_virtio_net_header_posts_used_and_raises_msix() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let hdr = 0x4000_4000;
    let payload = 0x4000_5000;
    let frame = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00payload";

    setup_queue(&mut dev, &mut mem, 1, desc, avail, used, 1);
    program_msix_vector(&mut dev, 1, 0xfee0_0000, 0x51);
    mem.write(hdr, &[0; VIRTIO_NET_HDR_LEN]);
    mem.write(payload, frame);
    write_desc(
        &mut mem,
        desc,
        0,
        hdr,
        VIRTIO_NET_HDR_LEN as u32,
        DESC_F_NEXT,
        1,
    );
    write_desc(&mut mem, desc, 1, payload, frame.len() as u32, 0, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());

    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

    assert_eq!(dev.backend().transmitted_frames(), &[frame.to_vec()]);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
        0
    );
    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 1,
            address: 0xfee0_0000,
            data: 0x51,
        }]
    );
}

#[test]
fn tx_notify_reuses_descriptor_and_packet_scratch_across_frames() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let hdr = 0x4000_4000;
    let payload = 0x4000_5000;
    let frame1 = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00first";
    let frame2 = b"\x02\x00\x00\x00\x00\x01\x52\x54\x00\x42\x56\x01\x08\x00again";

    setup_queue(&mut dev, &mut mem, 1, desc, avail, used, 1);
    mem.write(hdr, &[0; VIRTIO_NET_HDR_LEN]);
    mem.write(payload, frame1);
    write_desc(
        &mut mem,
        desc,
        0,
        hdr,
        VIRTIO_NET_HDR_LEN as u32,
        DESC_F_NEXT,
        1,
    );
    write_desc(&mut mem, desc, 1, payload, frame1.len() as u32, 0, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());

    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

    let desc_cap = dev.net.descriptor_scratch.capacity();
    let desc_ptr = dev.net.descriptor_scratch.as_ptr();
    let packet_cap = dev.net.tx_packet_scratch.capacity();
    let packet_ptr = dev.net.tx_packet_scratch.as_ptr();
    assert!(desc_cap >= 2);
    assert!(packet_cap >= VIRTIO_NET_HDR_LEN + frame1.len());

    mem.write(hdr, &[0; VIRTIO_NET_HDR_LEN]);
    mem.write(payload, frame2);
    write_desc(
        &mut mem,
        desc,
        2,
        hdr,
        VIRTIO_NET_HDR_LEN as u32,
        DESC_F_NEXT,
        3,
    );
    write_desc(&mut mem, desc, 3, payload, frame2.len() as u32, 0, 0);
    mem.write(avail + 2, &2u16.to_le_bytes());
    mem.write(avail + 6, &2u16.to_le_bytes());

    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET + 4, 4, 0, &mut mem);

    assert_eq!(
        dev.backend().transmitted_frames(),
        &[frame1.to_vec(), frame2.to_vec()]
    );
    assert_eq!(dev.net.descriptor_scratch.capacity(), desc_cap);
    assert_eq!(dev.net.descriptor_scratch.as_ptr(), desc_ptr);
    assert_eq!(dev.net.tx_packet_scratch.capacity(), packet_cap);
    assert_eq!(dev.net.tx_packet_scratch.as_ptr(), packet_ptr);
}

#[test]
fn tx_chain_rejects_oversized_guest_length_before_growing_scratch() {
    let mut mem = TestMem::new(0x4000_0000, 0x1000);
    let desc_table = 0x4000_0100;
    write_desc(&mut mem, desc_table, 0, 0x4000_0800, u32::MAX, 0, 0);
    let mut queue = VirtioNetQueue::new(0);
    queue.size = 1;
    queue.desc = desc_table;
    let mut descs = Vec::new();
    let mut packet = Vec::with_capacity(32);
    let capacity = packet.capacity();

    assert!(!VirtioNet::<LoopbackTestBackend>::tx_frame_from_chain_into(
        &mem,
        &queue,
        0,
        &mut descs,
        &mut packet,
    ));
    assert!(packet.is_empty());
    assert_eq!(packet.capacity(), capacity);
}

#[test]
fn rx_pump_prepends_header_posts_used_and_raises_msix() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let buf = 0x4000_4000;
    let frame = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00hello";

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x50);
    write_desc(&mut mem, desc, 0, buf, 128, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame.to_vec());

    assert!(dev.pump_receive(&mut mem));

    let packet = mem.read(buf, VIRTIO_NET_HDR_LEN + frame.len());
    assert_eq!(&packet[0..10], &[0; 10]);
    assert_eq!(&packet[10..12], &1u16.to_le_bytes());
    assert_eq!(&packet[VIRTIO_NET_HDR_LEN..], frame);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 4, 4).try_into().unwrap()),
        0
    );
    assert_eq!(
        u32::from_le_bytes(mem.read(used + 8, 4).try_into().unwrap()),
        (VIRTIO_NET_HDR_LEN + frame.len()) as u32
    );
    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 0,
            address: 0xfee0_0000,
            data: 0x50,
        }]
    );
}

#[test]
fn rx_pending_msix_survives_until_vector_is_programmed() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let buf = 0x4000_4000;
    let frame = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00late-vector";

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    write_desc(&mut mem, desc, 0, buf, 128, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame.to_vec());

    assert!(dev.pump_receive(&mut mem));
    assert!(dev.stats().queues[0].pending_msix);
    assert_eq!(dev.drain_pending_msix(true, false), Vec::new());
    assert!(dev.stats().queues[0].pending_msix);

    program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x50);

    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 0,
            address: 0xfee0_0000,
            data: 0x50,
        }]
    );
    assert!(!dev.stats().queues[0].pending_msix);
}

#[test]
fn rx_pump_reuses_descriptor_scratch_across_frames_without_packet_copy() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let buf1 = 0x4000_4000;
    let buf2 = 0x4000_5000;
    let frame1 = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00one";
    let frame2 = b"\x52\x54\x00\x42\x56\x01\x02\x00\x00\x00\x00\x01\x08\x00two";

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    write_desc(&mut mem, desc, 0, buf1, 128, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame1.to_vec());

    assert!(dev.pump_receive(&mut mem));

    let desc_cap = dev.net.descriptor_scratch.capacity();
    let desc_ptr = dev.net.descriptor_scratch.as_ptr();
    assert!(desc_cap >= 1);

    write_desc(&mut mem, desc, 1, buf2, 128, DESC_F_WRITE, 0);
    mem.write(avail + 2, &2u16.to_le_bytes());
    mem.write(avail + 6, &1u16.to_le_bytes());
    dev.backend_mut().push_receive(frame2.to_vec());

    assert!(dev.pump_receive(&mut mem));

    assert_eq!(
        &mem.read(buf1 + VIRTIO_NET_HDR_LEN as u64, frame1.len()),
        frame1
    );
    assert_eq!(
        &mem.read(buf2 + VIRTIO_NET_HDR_LEN as u64, frame2.len()),
        frame2
    );
    assert_eq!(dev.net.descriptor_scratch.capacity(), desc_cap);
    assert_eq!(dev.net.descriptor_scratch.as_ptr(), desc_ptr);
}

#[test]
fn rx_pump_reuses_backend_receive_scratch_across_frames() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let buf1 = 0x4000_4000;
    let buf2 = 0x4000_5000;
    let frame1 = [0x33u8; 96];
    let frame2 = [0x44u8; 64];

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    write_desc(&mut mem, desc, 0, buf1, 160, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame1);

    assert!(dev.pump_receive(&mut mem));
    let rx_cap = dev.net.rx_frame_scratch.capacity();
    let rx_ptr = dev.net.rx_frame_scratch.as_ptr();
    assert!(rx_cap >= frame1.len());
    assert!(dev.net.rx_frame_scratch.is_empty());

    write_desc(&mut mem, desc, 1, buf2, 160, DESC_F_WRITE, 0);
    mem.write(avail + 2, &2u16.to_le_bytes());
    mem.write(avail + 6, &1u16.to_le_bytes());
    dev.backend_mut().push_receive(frame2);

    assert!(dev.pump_receive(&mut mem));
    assert_eq!(dev.net.rx_frame_scratch.capacity(), rx_cap);
    assert_eq!(dev.net.rx_frame_scratch.as_ptr(), rx_ptr);
    assert!(dev.net.rx_frame_scratch.is_empty());
    assert_eq!(
        &mem.read(buf1 + VIRTIO_NET_HDR_LEN as u64, frame1.len()),
        &frame1
    );
    assert_eq!(
        &mem.read(buf2 + VIRTIO_NET_HDR_LEN as u64, frame2.len()),
        &frame2
    );
}

#[test]
fn rx_without_buffers_holds_one_frame_until_buffer_is_posted() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let buf = 0x4000_4000;
    let frame = b"\xaa\xbb\xcc\xdd";

    dev.backend_mut().push_receive(frame.to_vec());
    assert!(!dev.pump_receive(&mut mem));
    assert!(dev.stats().pending_rx_frame);
    assert!(dev.net.rx_frame_scratch.is_empty());

    setup_queue(&mut dev, &mut mem, 0, desc, avail, used, 0);
    write_desc(&mut mem, desc, 0, buf, 64, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());

    assert!(dev.pump_receive(&mut mem));
    assert!(!dev.stats().pending_rx_frame);
    assert!(dev.net.rx_frame_scratch.capacity() >= frame.len());
    assert!(dev.net.rx_frame_scratch.is_empty());
    assert_eq!(
        &mem.read(buf + VIRTIO_NET_HDR_LEN as u64, frame.len()),
        frame
    );
}
