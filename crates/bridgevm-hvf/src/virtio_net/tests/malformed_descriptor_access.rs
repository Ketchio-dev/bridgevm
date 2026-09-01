//! Regressions for malformed descriptor access directions on TX and RX chains.

use super::super::*;
use super::helpers::*;
use crate::msix::MsixMessage;

#[test]
fn tx_writable_interior_descriptor_rejects_complete_gather_and_clears_scratch() {
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let table = 0x4000_1000;
    let header = 0x4000_2000;
    let middle = 0x4000_2100;
    let suffix = 0x4000_2200;
    let mut expected = vec![0u8; VIRTIO_NET_HDR_LEN];
    expected.extend_from_slice(b"middle-suffix");
    mem.write(header, &[0; VIRTIO_NET_HDR_LEN]);
    mem.write(middle, b"middle-");
    mem.write(suffix, b"suffix");
    write_desc(
        &mut mem,
        table,
        0,
        header,
        VIRTIO_NET_HDR_LEN as u32,
        DESC_F_NEXT,
        1,
    );
    write_desc(&mut mem, table, 1, middle, 7, DESC_F_NEXT, 2);
    write_desc(&mut mem, table, 2, suffix, 6, 0, 0);
    let mut queue = VirtioNetQueue::new(0);
    queue.size = 3;
    queue.ready = true;
    queue.desc = table;
    let mut descs = Vec::new();
    let mut packet = Vec::new();

    assert!(VirtioNet::<LoopbackTestBackend>::tx_frame_from_chain_into(
        &mem,
        &queue,
        0,
        &mut descs,
        &mut packet,
    ));
    assert_eq!(packet, expected);
    let desc_capacity = descs.capacity();
    let packet_capacity = packet.capacity();

    write_desc(&mut mem, table, 1, middle, 7, DESC_F_WRITE | DESC_F_NEXT, 2);
    assert!(!VirtioNet::<LoopbackTestBackend>::tx_frame_from_chain_into(
        &mem,
        &queue,
        0,
        &mut descs,
        &mut packet,
    ));
    assert!(packet.is_empty());
    assert_eq!(descs.capacity(), desc_capacity);
    assert_eq!(packet.capacity(), packet_capacity);
}

#[test]
fn rx_read_only_interior_descriptor_rejects_before_first_buffer_write() {
    let mut dev = VirtioPciNet::new_loopback();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let table = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let first = 0x4000_4000;
    let middle = 0x4000_4100;
    let last = 0x4000_4200;
    let frame = b"split-rx-frame";

    setup_queue(&mut dev, &mut mem, QUEUE_RX as u16, table, avail, used, 0);
    program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x50);
    write_desc(&mut mem, table, 0, first, 8, DESC_F_WRITE | DESC_F_NEXT, 1);
    write_desc(&mut mem, table, 1, middle, 4, DESC_F_WRITE | DESC_F_NEXT, 2);
    write_desc(&mut mem, table, 2, last, 64, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame);

    assert!(dev.pump_receive(&mut mem));
    assert_eq!(mem.read(first, 8), [0; 8]);
    assert_eq!(mem.read(middle, 4), [0, 0, 1, 0]);
    assert_eq!(mem.read(last, frame.len()), frame);
    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 0,
            address: 0xfee0_0000,
            data: 0x50,
        }]
    );
    dev.net.interrupt_status = 0;

    let first_before = [0xa5; 8];
    mem.write(first, &first_before);
    write_desc(&mut mem, table, 1, middle, 4, DESC_F_NEXT, 2);
    mem.write(avail + 2, &2u16.to_le_bytes());
    mem.write(avail + 6, &0u16.to_le_bytes());
    dev.backend_mut().push_receive(frame);

    assert!(!dev.pump_receive(&mut mem));
    assert_eq!(mem.read(first, first_before.len()), first_before);
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(dev.stats().queues[QUEUE_RX].last_avail_idx, 1);
    assert_eq!(dev.stats().rx_count, 1);
    assert!(!dev.interrupt_line_level());
    assert!(dev.drain_pending_msix(true, false).is_empty());
}
