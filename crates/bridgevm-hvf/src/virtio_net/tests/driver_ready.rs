//! DRIVER_OK gates modern-device queue consumption, independently of queue enable.
use super::super::*;
use super::driver_ready_fixture::*;
use super::helpers::*;

#[test]
fn rx_waits_for_driver_ok_before_consuming_prepopulated_buffers() {
    let (mut dev, mut mem) = fixture();
    let before = mem.bytes.clone();
    dev.backend_mut().push_receive(FRAME);
    assert!(!dev.pump_receive(&mut mem), "RX must wait for DRIVER_OK");
    assert_eq!(mem.bytes, before);
    assert_quiet(&mut dev, &mem);
    ready(&mut dev, &mut mem);
    assert!(dev.pump_receive(&mut mem));
    assert_eq!(used_index(&mem, RX_USED), 1);
    assert_eq!(
        mem.read(RX_DATA + VIRTIO_NET_HDR_LEN as u64, FRAME.len()),
        FRAME
    );
    assert_eq!(dev.drain_pending_msix(true, false).len(), 1);
    assert!(!dev.pump_receive(&mut mem));
}

#[test]
fn premature_tx_doorbell_does_not_consume_or_transmit() {
    let (mut dev, mut mem) = fixture();
    let before = mem.bytes.clone();
    // A premature modern-driver notify is invalid, but cannot authorize DMA.
    tx_notify(&mut dev, &mut mem);
    assert!(dev.backend().transmitted_frames().is_empty());
    assert_eq!(mem.bytes, before);
    assert_quiet(&mut dev, &mem);
    ready(&mut dev, &mut mem);
    tx_notify(&mut dev, &mut mem);
    assert_eq!(dev.backend().transmitted_frames(), &[FRAME.to_vec()]);
    assert_eq!(used_index(&mem, TX_USED), 1);
    assert_eq!(dev.drain_pending_msix(true, false).len(), 1);
}

#[test]
fn ready_device_completes_both_queues_once() {
    let (mut dev, mut mem) = fixture();
    ready(&mut dev, &mut mem);
    dev.backend_mut().push_receive(FRAME);
    assert!(dev.pump_receive(&mut mem));
    tx_notify(&mut dev, &mut mem);
    assert_eq!(
        (used_index(&mem, RX_USED), used_index(&mem, TX_USED)),
        (1, 1)
    );
    assert_eq!((dev.stats().rx_count, dev.stats().tx_count), (1, 1));
    assert_eq!(dev.drain_pending_msix(true, false).len(), 2);
    assert!(!dev.pump_receive(&mut mem));
    tx_notify(&mut dev, &mut mem);
    assert_eq!((dev.stats().rx_count, dev.stats().tx_count), (1, 1));
}

#[test]
fn reset_requires_driver_ok_again_after_queue_reinitialization() {
    let (mut dev, mut mem) = fixture();
    ready(&mut dev, &mut mem);
    dev.backend_mut().push_receive(FRAME);
    assert!(dev.pump_receive(&mut mem));
    tx_notify(&mut dev, &mut mem);
    initialize(&mut dev, &mut mem);
    assert_quiet(&mut dev, &mem);
    dev.backend_mut().push_receive(FRAME);
    assert!(!dev.pump_receive(&mut mem));
    tx_notify(&mut dev, &mut mem);
    assert_quiet(&mut dev, &mem);
    ready(&mut dev, &mut mem);
    assert!(dev.pump_receive(&mut mem));
    tx_notify(&mut dev, &mut mem);
    assert_eq!((dev.stats().rx_count, dev.stats().tx_count), (1, 1));
    assert_eq!(dev.backend().transmitted_frames().len(), 2);
}

#[test]
fn driver_ok_does_not_override_disabled_queues() {
    let (mut dev, mut mem) = fixture();
    for queue in [0, 1] {
        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, queue, &mut mem);
        pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 0, &mut mem);
    }
    ready(&mut dev, &mut mem);
    dev.backend_mut().push_receive(FRAME);
    assert!(!dev.pump_receive(&mut mem));
    tx_notify(&mut dev, &mut mem);
    assert_quiet(&mut dev, &mem);
}
