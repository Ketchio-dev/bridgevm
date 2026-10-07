use super::super::agent_inbound::MAX_HOST_INBOUND_LEN;
use super::super::*;
use super::helpers::*;

const RING: u16 = 8;

fn ring_with_repeated_chain(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    queue: u16,
    data: u64,
    len: u32,
) -> (u64, u64) {
    let (desc, avail, used) = (0x4000_1000, 0x4000_2000, 0x4000_3000);
    setup_queue(dev, mem, queue, desc, avail, used, queue);
    for slot in 0..RING {
        write_desc(mem, desc, slot, data, len, 0, 0);
        mem.write(avail + 4 + u64::from(slot) * 2, &slot.to_le_bytes());
    }
    (avail, used)
}

fn kick(dev: &mut VirtioPciConsole, mem: &mut TestMem, queue: u16, avail: u64, idx: u16) {
    mem.write(avail + 2, &idx.to_le_bytes());
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET + u64::from(queue) * 4, 4, 0, mem);
}

#[test]
fn device_ready_flood_without_control_rx_buffers_keeps_a_bounded_backlog() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let data = 0x4000_8000;
    mem.write(data, &control_bytes(0, VIRTIO_CONSOLE_DEVICE_READY, 1));
    let queue = QUEUE_CONTROL_TX as u16;
    let (avail, _) = ring_with_repeated_chain(&mut dev, &mut mem, queue, data, 8);

    let mut idx = 0u16;
    for _ in 0..1000 {
        idx = idx.wrapping_add(RING);
        kick(&mut dev, &mut mem, queue, avail, idx);
    }

    // Every chain was consumed, but the undeliverable DEVICE_ADD replies stop
    // queueing at the backlog bound instead of growing per guest message.
    assert_eq!(
        dev.stats().queues[QUEUE_CONTROL_TX].used_produced,
        1000 * u64::from(RING)
    );
    assert_eq!(dev.stats().pending_control, MAX_PENDING_CONTROL_MESSAGES);
}

#[test]
fn undrained_agent_tx_stops_consuming_at_the_inbound_bound_and_resumes() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let data = 0x4000_8000;
    let chunk = MAX_AGENT_MESSAGE_LEN;
    mem.write(data, &vec![b'x'; chunk]);
    let queue = QUEUE_AGENT_TX as u16;
    let (avail, _) = ring_with_repeated_chain(&mut dev, &mut mem, queue, data, chunk as u32);

    let mut idx = 0u16;
    for _ in 0..1000 {
        idx = idx.wrapping_add(RING);
        kick(&mut dev, &mut mem, queue, avail, idx);
    }

    // Consumption stops once the bound is reached; the rest stays in the ring.
    let chains_to_fill = MAX_HOST_INBOUND_LEN / chunk;
    assert_eq!(dev.stats().host_inbound_len, MAX_HOST_INBOUND_LEN);
    assert_eq!(
        dev.stats().queues[QUEUE_AGENT_TX].used_produced,
        chains_to_fill as u64
    );

    // Draining the host side lets the next poll consume the waiting chains.
    let mut out = Vec::new();
    dev.drain_inbound_into(&mut out);
    assert_eq!(out.len(), MAX_HOST_INBOUND_LEN);
    assert!(dev.console.poll(&mut mem));
    assert_eq!(
        dev.stats().queues[QUEUE_AGENT_TX].used_produced,
        chains_to_fill as u64 + u64::from(RING)
    );
}
