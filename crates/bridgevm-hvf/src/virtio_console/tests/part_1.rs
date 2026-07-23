//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::msix::MsixMessage;
use std::collections::VecDeque;

#[test]
fn feature_negotiation_advertises_version_1_and_multiport_and_masks_driver_bits() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x1000);

    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_CONSOLE_F_MULTIPORT)
    );
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_F_VERSION_1)
    );
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);

    assert_eq!(
        dev.stats().driver_features,
        u64::from(VIRTIO_CONSOLE_F_MULTIPORT) | (u64::from(VIRTIO_F_VERSION_1) << 32)
    );
    assert_eq!(pci_read(&mut dev, COMMON_NUM_QUEUES, 2, &mut mem), 6);
    assert_eq!(
        pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 4, 4, &mut mem),
        2
    );
}

#[test]
fn full_control_handshake_emits_add_name_and_host_open() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x40000);
    let crx_desc = 0x4000_1000;
    let crx_avail = 0x4000_2000;
    let crx_used = 0x4000_3000;
    let ctx_desc = 0x4000_4000;
    let ctx_avail = 0x4000_5000;
    let ctx_used = 0x4000_6000;
    let out0 = 0x4000_7000;
    let out1 = 0x4000_7100;
    let out2 = 0x4000_7200;
    let out3 = 0x4000_7300;

    setup_queue(&mut dev, &mut mem, 2, crx_desc, crx_avail, crx_used, 2);
    setup_queue(&mut dev, &mut mem, 3, ctx_desc, ctx_avail, ctx_used, 3);
    for (idx, out) in [out0, out1, out2, out3].into_iter().enumerate() {
        post_rx(&mut mem, crx_desc, crx_avail, out, 64, idx as u16);
    }
    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(ctx_desc, ctx_avail),
        0x4000_8000,
        &control_bytes(0, VIRTIO_CONSOLE_DEVICE_READY, 1),
        0,
    );

    assert_eq!(
        mem.read(out0, 8),
        control_bytes(0, VIRTIO_CONSOLE_DEVICE_ADD, 0)
    );
    assert_eq!(
        mem.read(out1, 8),
        control_bytes(1, VIRTIO_CONSOLE_DEVICE_ADD, 0)
    );
    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(ctx_desc, ctx_avail),
        0x4000_8100,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
        1,
    );
    let mut expected_name = control_bytes(1, VIRTIO_CONSOLE_PORT_NAME, 0).to_vec();
    expected_name.extend_from_slice(AGENT_PORT_NAME);
    assert_eq!(mem.read(out2, expected_name.len()), expected_name);
    assert_eq!(
        u32::from_le_bytes(mem.read(crx_used + 4 + 2 * 8 + 4, 4).try_into().unwrap()),
        expected_name.len() as u32
    );
    assert_eq!(
        mem.read(out3, 8),
        control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
    );
    assert!(dev.stats().queues[2].pending_msix);
    assert!(dev.stats().queues[3].pending_msix);
}

#[test]
fn vioser_sequence_resends_host_open_before_agent_tx_without_guest_open_control() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x70000);
    let crx_desc = 0x4000_1000;
    let crx_avail = 0x4000_2000;
    let crx_used = 0x4000_3000;
    let ctx_desc = 0x4000_4000;
    let ctx_avail = 0x4000_5000;
    let ctx_used = 0x4000_6000;
    let atx_desc = 0x4000_7000;
    let atx_avail = 0x4000_8000;
    let atx_used = 0x4000_9000;

    setup_queue(&mut dev, &mut mem, 2, crx_desc, crx_avail, crx_used, 2);
    setup_queue(&mut dev, &mut mem, 3, ctx_desc, ctx_avail, ctx_used, 3);
    setup_queue(&mut dev, &mut mem, 5, atx_desc, atx_avail, atx_used, 5);
    for (idx, out) in [
        0x4000_a000,
        0x4000_a100,
        0x4000_a200,
        0x4000_a300,
        0x4000_a400,
        0x4000_a500,
    ]
    .into_iter()
    .enumerate()
    {
        post_rx(&mut mem, crx_desc, crx_avail, out, 64, idx as u16);
    }

    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(ctx_desc, ctx_avail),
        0x4000_b000,
        &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
        0,
    );
    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(ctx_desc, ctx_avail),
        0x4000_b100,
        &control_bytes(0, VIRTIO_CONSOLE_PORT_READY, 1),
        1,
    );
    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(ctx_desc, ctx_avail),
        0x4000_b200,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
        2,
    );

    assert!(!dev.stats().port1_guest_open);
    let mut expected_name = control_bytes(1, VIRTIO_CONSOLE_PORT_NAME, 0).to_vec();
    expected_name.extend_from_slice(AGENT_PORT_NAME);
    assert_eq!(mem.read(0x4000_a200, expected_name.len()), expected_name);
    assert_eq!(
        u32::from_le_bytes(mem.read(crx_used + 4 + 2 * 8 + 4, 4).try_into().unwrap()),
        expected_name.len() as u32
    );
    assert_eq!(
        mem.read(0x4000_a300, 8),
        control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
    );

    // The host retry heartbeat (any host->guest send) re-asserts PORT_OPEN
    // while the link is still unconfirmed. A bare control-RX rearm must NOT
    // (that path storms; see control_rx_notifies_alone_never_reassert...).
    pci_write(
        &mut dev,
        PCI_NOTIFY_CFG_OFFSET + u64::from(QUEUE_CONTROL_RX as u16) * 4,
        4,
        0,
        &mut mem,
    );
    assert_eq!(
        mem.read(0x4000_a400, 8),
        [0u8; 8],
        "a control-RX notify alone must not synthesize a PORT_OPEN"
    );
    dev.agent_send(b"PING");
    dev.poll(&mut mem);
    // The retry heartbeat re-sends PORT_NAME (in case vioser dropped the
    // first) followed by PORT_OPEN, so the next two RX buffers carry the pair.
    let mut expected_name = control_bytes(1, VIRTIO_CONSOLE_PORT_NAME, 0).to_vec();
    expected_name.extend_from_slice(AGENT_PORT_NAME);
    assert_eq!(mem.read(0x4000_a400, expected_name.len()), expected_name);
    assert_eq!(
        mem.read(0x4000_a500, 8),
        control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1)
    );

    send_tx(
        &mut dev,
        &mut mem,
        5,
        TestRing::new(atx_desc, atx_avail),
        0x4000_c000,
        b"READY",
        0,
    );
    assert_eq!(dev.take_inbound(), b"READY");
}

#[test]
fn data_loopback_after_port_open() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x50000);
    setup_queue(
        &mut dev,
        &mut mem,
        4,
        0x4000_1000,
        0x4000_2000,
        0x4000_3000,
        4,
    );
    setup_queue(
        &mut dev,
        &mut mem,
        5,
        0x4000_4000,
        0x4000_5000,
        0x4000_6000,
        5,
    );
    dev.console.ports[1].guest_open = true;

    post_rx(&mut mem, 0x4000_1000, 0x4000_2000, 0x4000_7000, 16, 0);
    dev.agent_send(b"ping");
    assert!(dev.poll(&mut mem));
    assert_eq!(mem.read(0x4000_7000, 4), b"ping");

    send_tx(
        &mut dev,
        &mut mem,
        5,
        TestRing::new(0x4000_4000, 0x4000_5000),
        0x4000_8000,
        b"pong",
        0,
    );
    assert_eq!(dev.take_inbound(), b"pong");
}

#[test]
fn drain_inbound_into_preserves_buffers() {
    let mut dev = VirtioPciConsole::new();
    dev.console.host_inbound.reserve(64);
    dev.console.host_inbound.extend_from_slice(b"READY\nPONG\n");
    let internal_capacity = dev.console.host_inbound.capacity();

    let mut out = Vec::with_capacity(32);
    let out_capacity = out.capacity();
    out.extend_from_slice(b"prefix:");
    dev.drain_inbound_into(&mut out);

    assert_eq!(out, b"prefix:READY\nPONG\n");
    assert_eq!(dev.console.host_inbound.len(), 0);
    assert_eq!(dev.console.host_inbound.capacity(), internal_capacity);
    assert_eq!(out.capacity(), out_capacity);

    dev.drain_inbound_into(&mut out);
    assert_eq!(out, b"prefix:READY\nPONG\n");
    assert_eq!(dev.console.host_inbound.capacity(), internal_capacity);
}

#[test]
fn tx_queues_reuse_descriptor_and_read_scratch_across_messages() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x70000);
    let ctx_desc = 0x4000_1000;
    let ctx_avail = 0x4000_2000;
    let ctx_used = 0x4000_3000;
    let atx_desc = 0x4000_4000;
    let atx_avail = 0x4000_5000;
    let atx_used = 0x4000_6000;

    setup_queue(&mut dev, &mut mem, 3, ctx_desc, ctx_avail, ctx_used, 3);
    setup_queue(&mut dev, &mut mem, 5, atx_desc, atx_avail, atx_used, 5);

    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(ctx_desc, ctx_avail),
        0x4000_7000,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1),
        0,
    );

    let desc_cap = dev.console.descriptor_scratch.capacity();
    let desc_ptr = dev.console.descriptor_scratch.as_ptr();
    let read_cap = dev.console.read_scratch.capacity();
    let read_ptr = dev.console.read_scratch.as_ptr();
    assert!(desc_cap >= 1);
    assert!(read_cap >= CONTROL_LEN);

    send_tx(
        &mut dev,
        &mut mem,
        5,
        TestRing::new(atx_desc, atx_avail),
        0x4000_7100,
        b"READY",
        0,
    );

    assert_eq!(dev.take_inbound(), b"READY");
    assert_eq!(dev.console.descriptor_scratch.capacity(), desc_cap);
    assert_eq!(dev.console.descriptor_scratch.as_ptr(), desc_ptr);
    assert_eq!(dev.console.read_scratch.capacity(), read_cap);
    assert_eq!(dev.console.read_scratch.as_ptr(), read_ptr);
}

#[test]
fn tx_chain_rejects_oversized_guest_length_before_growing_scratch() {
    let mut mem = TestMem::new(0x4000_0000, 0x1000);
    let desc_table = 0x4000_0100;
    write_desc(&mut mem, desc_table, 0, 0x4000_0800, u32::MAX, 0, 0);
    let mut queue = VirtioConsoleQueue::new(0);
    queue.size = 1;
    queue.desc = desc_table;
    let mut descs = Vec::new();
    let mut bytes = Vec::with_capacity(32);
    let capacity = bytes.capacity();

    assert!(!VirtioConsole::read_chain_into(
        &mem,
        &queue,
        0,
        &mut descs,
        &mut bytes,
        MAX_AGENT_MESSAGE_LEN,
    ));
    assert!(bytes.is_empty());
    assert_eq!(bytes.capacity(), capacity);
}

#[test]
fn agent_rx_delivers_wrapped_host_queue_and_reuses_descriptor_scratch() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x50000);
    let arx_desc = 0x4000_1000;
    let arx_avail = 0x4000_2000;
    let arx_used = 0x4000_3000;
    let out0 = 0x4000_4000;
    let out1 = 0x4000_4100;

    setup_queue(&mut dev, &mut mem, 4, arx_desc, arx_avail, arx_used, 4);
    post_rx(&mut mem, arx_desc, arx_avail, out0, 16, 0);
    let mut wrapped = VecDeque::with_capacity(8);
    wrapped.extend(b"ABCDEFGH".iter().copied());
    for _ in 0..6 {
        assert!(wrapped.pop_front().is_some());
    }
    wrapped.extend(b"IJKL".iter().copied());
    assert_eq!(wrapped.iter().copied().collect::<Vec<_>>(), b"GHIJKL");
    assert!(
        !wrapped.as_slices().0.is_empty() && !wrapped.as_slices().1.is_empty(),
        "test setup must exercise VecDeque's split-slice layout"
    );
    dev.console.host_to_guest = wrapped;

    assert!(dev.poll(&mut mem));
    assert_eq!(mem.read(out0, 6), b"GHIJKL");
    assert_eq!(dev.stats().host_to_guest_len, 0);

    let desc_cap = dev.console.descriptor_scratch.capacity();
    let desc_ptr = dev.console.descriptor_scratch.as_ptr();
    assert!(desc_cap >= 1);

    post_rx(&mut mem, arx_desc, arx_avail, out1, 16, 1);
    dev.agent_send(b"pong");

    assert!(dev.poll(&mut mem));
    assert_eq!(mem.read(out1, 4), b"pong");
    assert_eq!(dev.console.descriptor_scratch.capacity(), desc_cap);
    assert_eq!(dev.console.descriptor_scratch.as_ptr(), desc_ptr);
}

#[test]
fn agent_rx_pending_msix_survives_until_table_entry_is_programmed() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x50000);
    let arx_desc = 0x4000_1000;
    let arx_avail = 0x4000_2000;
    let arx_used = 0x4000_3000;
    let out = 0x4000_4000;

    setup_queue(&mut dev, &mut mem, 4, arx_desc, arx_avail, arx_used, 4);
    post_rx(&mut mem, arx_desc, arx_avail, out, 16, 0);
    dev.agent_send(b"PING");

    assert!(dev.poll(&mut mem));
    assert_eq!(mem.read(out, 4), b"PING");
    assert!(dev.stats().queues[4].pending_msix);
    assert_eq!(dev.drain_pending_msix(true, false), Vec::new());
    assert!(dev.stats().queues[4].pending_msix);

    program_msix_vector(&mut dev, 4, 0xfee0_0000, 0x54);

    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 4,
            address: 0xfee0_0000,
            data: 0x54,
        }]
    );
    assert!(!dev.stats().queues[4].pending_msix);
}

#[test]
fn control_backpressure_queues_until_rx_buffer_is_posted() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    setup_queue(
        &mut dev,
        &mut mem,
        2,
        0x4000_1000,
        0x4000_2000,
        0x4000_3000,
        2,
    );
    setup_queue(
        &mut dev,
        &mut mem,
        3,
        0x4000_4000,
        0x4000_5000,
        0x4000_6000,
        3,
    );

    send_tx(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(0x4000_4000, 0x4000_5000),
        0x4000_8000,
        &control_bytes(0, VIRTIO_CONSOLE_DEVICE_READY, 1),
        0,
    );
    assert_eq!(dev.stats().pending_control, 2);

    post_rx(&mut mem, 0x4000_1000, 0x4000_2000, 0x4000_7000, 16, 0);
    assert!(dev.poll(&mut mem));
    assert_eq!(
        mem.read(0x4000_7000, 8),
        control_bytes(0, VIRTIO_CONSOLE_DEVICE_ADD, 0)
    );
    assert_eq!(dev.stats().pending_control, 1);
}

#[test]
fn reset_clears_port_state_and_pending_buffers() {
    let mut dev = VirtioPciConsole::new();
    dev.console.ports[1].ready = true;
    dev.console.ports[1].guest_open = true;
    dev.console
        .pending_control
        .push_back(PendingControlMessage::from_slice(&[1, 2, 3]));
    dev.agent_send(b"ping");
    dev.console.host_inbound.extend_from_slice(b"pong");

    dev.reset_runtime_state();

    let stats = dev.stats();
    assert!(!stats.port1_ready);
    assert!(!stats.port1_guest_open);
    assert_eq!(stats.pending_control, 0);
    assert_eq!(stats.host_to_guest_len, 0);
    assert_eq!(stats.host_inbound_len, 0);
}

#[test]
fn guest_model_latches_host_connected_and_relatches_after_d0_bounce() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x100000);
    let size: u16 = 32;
    setup_queue_sized(
        &mut dev,
        &mut mem,
        2,
        TestRing::new(0x4000_1000, 0x4000_2000),
        0x4000_3000,
        2,
        size,
    );
    setup_queue_sized(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(0x4000_4000, 0x4000_5000),
        0x4000_6000,
        3,
        size,
    );
    for n in 0..24u16 {
        post_control_rx(
            &mut mem,
            0x4000_1000,
            0x4000_2000,
            0x4004_0000 + u64::from(n) * 0x100,
            size,
            n,
        );
    }
    let mut seen = 0u16;
    let mut guest = GuestModel::default();

    // Boot handshake exactly as vioser emits it: DEVICE_READY(BAD_ID),
    // then a PORT_READY per port as each PDO enters D0.
    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0000,
        &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
        0,
    );
    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0100,
        &control_bytes(0, VIRTIO_CONSOLE_PORT_READY, 1),
        1,
    );
    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0200,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
        2,
    );
    for message in drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen) {
        guest.apply(&message);
    }

    assert!(guest.present.contains(&1), "PORT_ADD must create port 1");
    assert!(guest.named.contains(&1), "PORT_NAME must resolve port 1");
    assert!(
        guest.host_connected(1),
        "PORT_OPEN must latch HostConnected in the same drain PORT_NAME resolved"
    );
    assert!(!guest.host_connected(0), "host never opens port 0");

    // A PnP resource rebalance / D0 bounce: vioser clears HostConnected in
    // VIOSerialPortEvtDeviceD0Exit, then re-enters D0 and re-emits
    // PORT_READY(1). The device must re-assert PORT_OPEN so the link heals.
    guest.host_conn.insert(1, false);
    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0300,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
        3,
    );
    for message in drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen) {
        guest.apply(&message);
    }
    assert!(
        guest.host_connected(1),
        "a fresh PORT_READY after a D0 bounce must re-latch HostConnected"
    );
}

#[test]
fn host_open_reassert_is_sustained_until_agent_tx_then_stops() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x100000);
    let size: u16 = 32;
    setup_queue_sized(
        &mut dev,
        &mut mem,
        2,
        TestRing::new(0x4000_1000, 0x4000_2000),
        0x4000_3000,
        2,
        size,
    );
    setup_queue_sized(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(0x4000_4000, 0x4000_5000),
        0x4000_6000,
        3,
        size,
    );
    setup_queue_sized(
        &mut dev,
        &mut mem,
        5,
        TestRing::new(0x4000_7000, 0x4000_8000),
        0x4000_9000,
        5,
        size,
    );
    for n in 0..28u16 {
        post_control_rx(
            &mut mem,
            0x4000_1000,
            0x4000_2000,
            0x4004_0000 + u64::from(n) * 0x100,
            size,
            n,
        );
    }
    let mut seen = 0u16;

    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0000,
        &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
        0,
    );
    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0100,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
        1,
    );
    let _ = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
    assert!(dev.stats().port1_host_open);
    assert!(!dev.stats().agent_connected_confirmed);

    // The host retry heartbeat (harness PINGs, or any host->guest send)
    // re-asserts PORT_OPEN while the link is still unconfirmed, so a port
    // that only stabilizes after the boot burst still latches. Exactly one
    // re-assert is in flight at a time (no control-queue flooding).
    let open = control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1).to_vec();
    for _ in 0..4 {
        dev.agent_send(b"PING");
        dev.poll(&mut mem);
        let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
        assert_eq!(
            delivered
                .iter()
                .filter(|message| message.as_slice() == open.as_slice())
                .count(),
            1,
            "each heartbeat re-asserts exactly one PORT_OPEN while unconfirmed"
        );
    }

    // First guest TX byte proves vioser latched HostConnected (its
    // WillWriteBlock gate blocks writes until then).
    send_tx(
        &mut dev,
        &mut mem,
        5,
        TestRing::new(0x4000_7000, 0x4000_8000),
        0x4006_0000,
        b"READY",
        0,
    );
    assert!(dev.stats().agent_connected_confirmed);
    assert_eq!(dev.take_inbound(), b"READY");

    // Re-assertion stops once the link is proven.
    dev.agent_send(b"PING");
    dev.poll(&mut mem);
    let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
    assert_eq!(
        delivered
            .iter()
            .filter(|message| message.as_slice() == open.as_slice())
            .count(),
        0,
        "re-assertion stops after the agent proves the link is live"
    );
}

#[test]
fn control_rx_notifies_alone_never_reassert_port_open_no_storm() {
    // Regression for the MSI-X storm: delivering a PORT_OPEN makes vioser
    // consume + refill the control-RX ring and kick control-RX. If that
    // kick re-asserts another PORT_OPEN, the cycle runs at full interrupt
    // speed and livelocks the guest. A bare control-RX notify must produce
    // no work at all so the loop cannot sustain itself.
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x100000);
    let size: u16 = 32;
    setup_queue_sized(
        &mut dev,
        &mut mem,
        2,
        TestRing::new(0x4000_1000, 0x4000_2000),
        0x4000_3000,
        2,
        size,
    );
    setup_queue_sized(
        &mut dev,
        &mut mem,
        3,
        TestRing::new(0x4000_4000, 0x4000_5000),
        0x4000_6000,
        3,
        size,
    );
    for n in 0..30u16 {
        post_control_rx(
            &mut mem,
            0x4000_1000,
            0x4000_2000,
            0x4004_0000 + u64::from(n) * 0x100,
            size,
            n,
        );
    }
    let mut seen = 0u16;

    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0000,
        &control_bytes(0xffff_ffff, VIRTIO_CONSOLE_DEVICE_READY, 1),
        0,
    );
    guest_control(
        &mut dev,
        &mut mem,
        0x4005_0100,
        &control_bytes(1, VIRTIO_CONSOLE_PORT_READY, 1),
        1,
    );
    let _ = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
    assert!(dev.stats().port1_host_open);
    assert!(!dev.stats().agent_connected_confirmed);
    assert_eq!(dev.stats().pending_control, 0);

    // Replay the vioser "consumed + refilled -> kick control-RX" many times
    // with no agent TX. Each kick must manufacture nothing: no control
    // message delivered, nothing queued, so nothing to interrupt on.
    let open = control_bytes(1, VIRTIO_CONSOLE_PORT_OPEN, 1).to_vec();
    for _ in 0..64 {
        pci_write(
            &mut dev,
            PCI_NOTIFY_CFG_OFFSET + u64::from(QUEUE_CONTROL_RX as u16) * 4,
            4,
            0,
            &mut mem,
        );
        let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
        assert!(
            delivered.is_empty(),
            "a bare control-RX notify must not deliver any control message"
        );
        assert_eq!(
            dev.stats().pending_control,
            0,
            "a bare control-RX notify must not enqueue a PORT_OPEN"
        );
    }

    // The bounded heartbeat trigger still re-asserts exactly once.
    dev.agent_send(b"PING");
    dev.poll(&mut mem);
    let delivered = drain_control_rx(&mem, 0x4000_1000, 0x4000_3000, size, &mut seen);
    assert_eq!(
        delivered
            .iter()
            .filter(|message| message.as_slice() == open.as_slice())
            .count(),
        1,
        "agent_send remains the bounded re-assert trigger"
    );
}
