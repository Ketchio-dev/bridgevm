//! Split test module.

use super::super::*;
use crate::fwcfg::GuestMemoryMut;
use crate::pcie::VIRTIO_CONSOLE_MSIX_TABLE_OFFSET;

#[derive(Debug)]
pub(super) struct TestMem {
    pub(super) base: u64,
    pub(super) bytes: Vec<u8>,
}

impl TestMem {
    pub(super) fn new(base: u64, len: usize) -> Self {
        Self {
            base,
            bytes: vec![0; len],
        }
    }

    pub(super) fn write(&mut self, gpa: u64, data: &[u8]) {
        assert!(self.write_bytes(gpa, data));
    }

    pub(super) fn read(&self, gpa: u64, len: usize) -> Vec<u8> {
        self.read_bytes(gpa, len).unwrap()
    }
}

impl GuestMemoryMut for TestMem {
    fn write_bytes(&mut self, gpa: u64, data: &[u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(data.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        self.bytes[off..end].copy_from_slice(data);
        true
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        let off = gpa.checked_sub(self.base)? as usize;
        let end = off.checked_add(len)?;
        (end <= self.bytes.len()).then(|| self.bytes[off..end].to_vec())
    }

    fn read_into(&self, gpa: u64, dst: &mut [u8]) -> bool {
        let Some(off) = gpa.checked_sub(self.base).map(|v| v as usize) else {
            return false;
        };
        let Some(end) = off.checked_add(dst.len()) else {
            return false;
        };
        if end > self.bytes.len() {
            return false;
        }
        dst.copy_from_slice(&self.bytes[off..end]);
        true
    }
}

pub(super) fn pci_write(
    dev: &mut VirtioPciConsole,
    offset: u64,
    size: u8,
    value: u64,
    mem: &mut TestMem,
) {
    assert_eq!(
        dev.access(offset, VirtioPciConsoleOp::Write { size, value }, mem),
        VirtioConsoleResult::WriteAck
    );
}

pub(super) fn pci_read(
    dev: &mut VirtioPciConsole,
    offset: u64,
    size: u8,
    mem: &mut TestMem,
) -> u64 {
    match dev.access(offset, VirtioPciConsoleOp::Read { size }, mem) {
        VirtioConsoleResult::ReadValue(value) => value,
        VirtioConsoleResult::WriteAck => panic!("read returned write ack"),
    }
}

pub(super) fn write_desc(
    mem: &mut TestMem,
    table: u64,
    index: u16,
    addr: u64,
    len: u32,
    flags: u16,
    next: u16,
) {
    let gpa = table + u64::from(index) * DESC_SIZE;
    mem.write(gpa, &addr.to_le_bytes());
    mem.write(gpa + 8, &len.to_le_bytes());
    mem.write(gpa + 12, &flags.to_le_bytes());
    mem.write(gpa + 14, &next.to_le_bytes());
}

pub(super) fn setup_queue(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    queue: u16,
    desc: u64,
    avail: u64,
    used: u64,
    vector: u16,
) {
    pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
    pci_write(dev, COMMON_QUEUE_SIZE, 2, 8, mem);
    pci_write(dev, COMMON_QUEUE_DESC, 8, desc, mem);
    pci_write(dev, COMMON_QUEUE_DRIVER, 8, avail, mem);
    pci_write(dev, COMMON_QUEUE_DEVICE, 8, used, mem);
    pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
    pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
}

pub(super) fn program_msix_vector(
    dev: &mut VirtioPciConsole,
    vector: u16,
    address: u64,
    data: u32,
) {
    let off = u64::from(VIRTIO_CONSOLE_MSIX_TABLE_OFFSET) + u64::from(vector) * 16;
    assert_eq!(
        dev.msix_bar_access(
            off,
            VirtioPciConsoleOp::Write {
                size: 8,
                value: address,
            },
        ),
        VirtioConsoleResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(
            off + 8,
            VirtioPciConsoleOp::Write {
                size: 4,
                value: u64::from(data),
            },
        ),
        VirtioConsoleResult::WriteAck
    );
    assert_eq!(
        dev.msix_bar_access(off + 12, VirtioPciConsoleOp::Write { size: 4, value: 0 },),
        VirtioConsoleResult::WriteAck
    );
}

pub(super) fn post_rx(mem: &mut TestMem, desc: u64, avail: u64, data: u64, len: u32, idx: u16) {
    write_desc(mem, desc, idx, data, len, DESC_F_WRITE, 0);
    mem.write(avail + 2, &idx.wrapping_add(1).to_le_bytes());
    mem.write(avail + 4 + u64::from(idx) * 2, &idx.to_le_bytes());
}

#[derive(Clone, Copy)]
pub(super) struct TestRing {
    pub(super) desc: u64,
    pub(super) avail: u64,
}

impl TestRing {
    pub(super) const fn new(desc: u64, avail: u64) -> Self {
        Self { desc, avail }
    }
}

pub(super) fn send_tx(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    queue: u16,
    ring: TestRing,
    data_addr: u64,
    data: &[u8],
    idx: u16,
) {
    let TestRing { desc, avail } = ring;
    mem.write(data_addr, data);
    write_desc(mem, desc, idx, data_addr, data.len() as u32, 0, 0);
    mem.write(avail + 2, &idx.wrapping_add(1).to_le_bytes());
    mem.write(avail + 4 + u64::from(idx) * 2, &idx.to_le_bytes());
    pci_write(dev, PCI_NOTIFY_CFG_OFFSET + u64::from(queue) * 4, 4, 0, mem);
}

pub(super) fn control_bytes(id: u32, event: u16, value: u16) -> [u8; 8] {
    Control::new(id, event, value).bytes()
}

// ---- Faithful guest (vioser) model ---------------------------------
//
// These tests replay the control messages our device delivers on the
// control-RX queue through a small state machine that mirrors the parts of
// vioser that gate the data plane: VIOSerialFindPortById (a port is only
// resolvable after PORT_ADD created it), the PORT_NAME name handler, and
// the PORT_OPEN -> HostConnected latch. Modeling the real driver contract
// (rather than poking device internals) is what catches the netkvm-class
// bug where bytes move but the driver-visible state never advances.

#[derive(Default)]
pub(super) struct GuestModel {
    pub(super) present: std::collections::BTreeSet<u32>,
    pub(super) named: std::collections::BTreeSet<u32>,
    pub(super) host_conn: std::collections::BTreeMap<u32, bool>,
}

impl GuestModel {
    pub(super) fn apply(&mut self, raw: &[u8]) {
        let control = Control::parse(raw).expect("control message");
        match control.event {
            VIRTIO_CONSOLE_DEVICE_ADD => {
                // vioser's PORT_ADD handler: the port becomes findable.
                self.present.insert(control.id);
                self.host_conn.entry(control.id).or_insert(false);
            }
            VIRTIO_CONSOLE_PORT_NAME if self.present.contains(&control.id) => {
                // VIOSerialPortCreateName only runs if the port resolves.
                self.named.insert(control.id);
            }
            VIRTIO_CONSOLE_PORT_OPEN if self.present.contains(&control.id) => {
                // VIOSerialHandleCtrlMsg PORT_OPEN: only latches when the
                // port resolves; value drives HostConnected.
                self.host_conn.insert(control.id, control.value != 0);
            }
            _ => {}
        }
    }

    pub(super) fn host_connected(&self, id: u32) -> bool {
        self.host_conn.get(&id).copied().unwrap_or(false)
    }
}

pub(super) fn setup_queue_sized(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    queue: u16,
    ring: TestRing,
    used: u64,
    vector: u16,
    size: u16,
) {
    let TestRing { desc, avail } = ring;
    pci_write(dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), mem);
    pci_write(dev, COMMON_QUEUE_SIZE, 2, u64::from(size), mem);
    pci_write(dev, COMMON_QUEUE_DESC, 8, desc, mem);
    pci_write(dev, COMMON_QUEUE_DRIVER, 8, avail, mem);
    pci_write(dev, COMMON_QUEUE_DEVICE, 8, used, mem);
    pci_write(dev, COMMON_QUEUE_MSIX_VECTOR, 2, u64::from(vector), mem);
    pci_write(dev, COMMON_QUEUE_ENABLE, 2, 1, mem);
}

pub(super) fn post_control_rx(
    mem: &mut TestMem,
    desc: u64,
    avail: u64,
    data: u64,
    size: u16,
    n: u16,
) {
    let slot = n % size;
    write_desc(mem, desc, slot, data, 64, DESC_F_WRITE, 0);
    mem.write(avail + 4 + u64::from(slot) * 2, &slot.to_le_bytes());
    mem.write(avail + 2, &n.wrapping_add(1).to_le_bytes());
}

/// Read every control-RX buffer the device has newly published, returning
/// the raw message bytes in order and advancing `seen`.
pub(super) fn drain_control_rx(
    mem: &TestMem,
    desc: u64,
    used: u64,
    size: u16,
    seen: &mut u16,
) -> Vec<Vec<u8>> {
    let used_idx = u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap());
    let mut messages = Vec::new();
    while *seen != used_idx {
        let slot = u64::from(*seen % size);
        let entry = used + 4 + slot * 8;
        let head = u32::from_le_bytes(mem.read(entry, 4).try_into().unwrap());
        let len = u32::from_le_bytes(mem.read(entry + 4, 4).try_into().unwrap()) as usize;
        let addr = u64::from_le_bytes(
            mem.read(desc + u64::from(head) * DESC_SIZE, 8)
                .try_into()
                .unwrap(),
        );
        messages.push(mem.read(addr, len));
        *seen = seen.wrapping_add(1);
    }
    messages
}

pub(super) fn guest_control(
    dev: &mut VirtioPciConsole,
    mem: &mut TestMem,
    data_addr: u64,
    message: &[u8],
    idx: u16,
) {
    send_tx(
        dev,
        mem,
        3,
        TestRing::new(0x4000_4000, 0x4000_5000),
        data_addr,
        message,
        idx,
    );
}

fn post_rx_buffer(
    mem: &mut TestMem,
    ring: TestRing,
    size: u16,
    avail_idx: u16,
    slot: u16,
    buf: u64,
    buf_len: u32,
) {
    let TestRing { desc, avail } = ring;
    write_desc(mem, desc, slot, buf, buf_len, DESC_F_WRITE, 0);
    mem.write(
        avail + 4 + u64::from(avail_idx % size) * 2,
        &slot.to_le_bytes(),
    );
    mem.write(avail + 2, &avail_idx.wrapping_add(1).to_le_bytes());
}

#[test]
fn agent_rx_delivery_resumes_after_ring_consumed_and_replenished() {
    // Replenishment regression for the "stops after the first lap" wall:
    // consume a full RX ring, then have the guest re-post buffers past the
    // initial size (wrapping the ring slots) with NO doorbell, and assert
    // the periodic poll picks them up. Exercises absolute-avail-idx tracking
    // across a lap and delivery via poll rather than only via notify.
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x100000);
    let size: u16 = 4;
    setup_queue_sized(
        &mut dev,
        &mut mem,
        4,
        TestRing::new(0x4000_1000, 0x4000_2000),
        0x4000_3000,
        4,
        size,
    );

    // Lap 1: guest fills the whole ring (avail 0..4, descriptor slots 0..4).
    for k in 0..4u16 {
        post_rx_buffer(
            &mut mem,
            TestRing::new(0x4000_1000, 0x4000_2000),
            size,
            k,
            k,
            0x4004_0000 + u64::from(k) * 0x100,
            64,
        );
    }

    // Consume the entire ring (one host->guest message per buffer).
    for k in 0..4u16 {
        dev.agent_send(format!("m{k}").as_bytes());
        dev.poll(&mut mem);
    }
    assert_eq!(dev.stats().queues[4].last_avail_idx, 4);
    assert_eq!(dev.stats().queues[4].used_produced, 4);
    assert_eq!(mem.read(0x4004_0000, 2), b"m0");
    assert_eq!(mem.read(0x4004_0300, 2), b"m3");

    // Ring drained: a further send finds no buffer and is retained.
    let no_buf_before = dev.stats().queues[4].rx_no_buffers;
    dev.agent_send(b"stall");
    dev.poll(&mut mem);
    assert!(dev.stats().queues[4].rx_no_buffers > no_buf_before);
    assert_eq!(
        dev.stats().host_to_guest_len,
        5,
        "undeliverable bytes are held"
    );

    // Lap 2: guest replenishes past the initial size (avail 4..8) reusing
    // ring slots 0..4, and does NOT ring the doorbell.
    for (i, k) in (4..8u16).enumerate() {
        post_rx_buffer(
            &mut mem,
            TestRing::new(0x4000_1000, 0x4000_2000),
            size,
            k,
            i as u16,
            0x4005_0000 + u64::from(i as u16) * 0x100,
            64,
        );
    }

    // The periodic poll (no notify) must resume delivery into lap-2 buffers.
    assert!(dev.poll(&mut mem), "poll must consume replenished buffers");
    assert_eq!(dev.stats().queues[4].last_avail_idx, 5);
    assert_eq!(mem.read(0x4005_0000, 5), b"stall");
    assert_eq!(dev.stats().host_to_guest_len, 0);
}
