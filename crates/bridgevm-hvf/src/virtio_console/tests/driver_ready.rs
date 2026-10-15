//! Public PCI control-queue readiness, separate from the agent-port handshake.
use super::super::*;
use super::driver_ready_fixture::*;
use super::helpers::*;

fn quiet(dev: &VirtioPciConsole, mem: &TestMem) {
    assert_eq!((used_index(mem, RX_USED), used_index(mem, TX_USED)), (0, 0));
    assert_eq!(dev.stats().pending_control, 0);
    assert!(dev.stats().queues.iter().all(|queue| {
        queue.last_avail_idx == 0 && queue.used_produced == 0 && !queue.pending_msix
    }));
    assert!(!dev.interrupt_line_level());
}

fn complete(dev: &mut VirtioPciConsole, mem: &mut TestMem) {
    assert_eq!((used_index(mem, RX_USED), used_index(mem, TX_USED)), (2, 1));
    assert_eq!(
        mem.read(RX_DATA, 8),
        control_bytes(0, VIRTIO_CONSOLE_DEVICE_ADD, 0)
    );
    assert_eq!(
        mem.read(RX_DATA + 0x100, 8),
        control_bytes(1, VIRTIO_CONSOLE_DEVICE_ADD, 0)
    );
    assert_eq!(dev.stats().queues[2].last_avail_idx, 2);
    assert_eq!(dev.stats().queues[3].last_avail_idx, 1);
    assert_eq!(dev.stats().pending_control, 0);
    assert!(dev.interrupt_line_level());
    assert!(!dev.poll(mem));
    assert_eq!((used_index(mem, RX_USED), used_index(mem, TX_USED)), (2, 1));
}

#[test]
fn automatic_poll_waits_for_driver_ok_before_control_handshake() {
    let (mut dev, mut mem) = fixture();
    let before = mem.bytes.clone();
    assert!(
        !dev.poll(&mut mem),
        "control queues must wait for DRIVER_OK"
    );
    assert_eq!(mem.bytes, before);
    quiet(&dev, &mem);
    ready(&mut dev, &mut mem);
    assert!(dev.poll(&mut mem));
    complete(&mut dev, &mut mem);
}

#[test]
fn premature_control_doorbell_does_not_consume_queues() {
    let (mut dev, mut mem) = fixture();
    let before = mem.bytes.clone();
    // Defensive invalid-driver control: notification before DRIVER_OK is forbidden.
    notify(&mut dev, &mut mem);
    assert_eq!(mem.bytes, before);
    quiet(&dev, &mem);
    ready(&mut dev, &mut mem);
    notify(&mut dev, &mut mem);
    complete(&mut dev, &mut mem);
}

#[test]
fn ready_control_handshake_completes_once() {
    let (mut dev, mut mem) = fixture();
    ready(&mut dev, &mut mem);
    assert!(dev.poll(&mut mem));
    complete(&mut dev, &mut mem);
}

#[test]
fn reset_reinitialized_control_queues_wait_for_driver_ok_again() {
    let (mut dev, mut mem) = fixture();
    ready(&mut dev, &mut mem);
    assert!(dev.poll(&mut mem));
    initialize(&mut dev, &mut mem);
    let before = mem.bytes.clone();
    quiet(&dev, &mem);
    assert!(!dev.poll(&mut mem));
    assert_eq!(mem.bytes, before);
    quiet(&dev, &mem);
    ready(&mut dev, &mut mem);
    assert!(dev.poll(&mut mem));
    complete(&mut dev, &mut mem);
}
