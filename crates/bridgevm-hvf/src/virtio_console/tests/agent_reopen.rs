//! An agent restart without a guest reboot: the process dies, vioser's VIOSerialPortClose
//! sends PORT_OPEN 0, and the restarted agent's VIOSerialPortCreate sends PORT_OPEN 1. No
//! PORT_READY follows, so the host must keep both directions working without a new handshake.
//! vioser's HostConnected (the gate on guest reads and writes) changes only on a host
//! PORT_OPEN or a D0 exit, so the model below keeps it across a guest close.

use super::super::*;
use super::helpers::*;
use super::ready_queue::*;

const CONTROL_RX: TestRing = TestRing::new(0x4000_1000, 0x4000_2000);
const CONTROL_RX_USED: u64 = 0x4000_3000;
const CONTROL_TX: TestRing = TestRing::new(0x4000_4000, 0x4000_5000);
const AGENT_RX: TestRing = TestRing::new(0x4000_7000, 0x4000_8000);
const AGENT_RX_USED: u64 = 0x4000_9000;
const AGENT_TX: TestRing = TestRing::new(0x4000_a000, 0x4000_b000);
const SIZE: u16 = 32;

struct Guest {
    model: GuestModel,
    control_seen: u16,
    control_tx: u16,
    agent_tx: u16,
    agent_rx_posted: u16,
}

impl Guest {
    /// Apply every control message the device has published since the last call.
    fn sync(&mut self, mem: &TestMem) {
        let messages = drain_control_rx(
            mem,
            CONTROL_RX.desc,
            CONTROL_RX_USED,
            SIZE,
            &mut self.control_seen,
        );
        for message in messages {
            self.model.apply(&message);
        }
    }

    fn control(
        &mut self,
        dev: &mut VirtioPciConsole,
        mem: &mut TestMem,
        id: u32,
        event: u16,
        value: u16,
    ) {
        let addr = 0x4005_0000 + u64::from(self.control_tx) * 0x100;
        guest_control(
            dev,
            mem,
            addr,
            &control_bytes(id, event, value),
            self.control_tx,
        );
        self.control_tx += 1;
        self.sync(mem);
    }

    /// The agent's WriteFile; vioser's VIOSerialWillWriteBlock holds it while !HostConnected.
    fn write(&mut self, dev: &mut VirtioPciConsole, mem: &mut TestMem, line: &[u8]) -> bool {
        if !self.model.host_connected(AGENT_PORT_ID) {
            return false;
        }
        let addr = 0x4006_0000 + u64::from(self.agent_tx) * 0x100;
        send_tx(
            dev,
            mem,
            QUEUE_AGENT_TX as u16,
            AGENT_TX,
            addr,
            line,
            self.agent_tx,
        );
        self.agent_tx += 1;
        true
    }

    /// One receive buffer; returns what the device delivered into it after the host's send.
    fn read(
        &mut self,
        dev: &mut VirtioPciConsole,
        mem: &mut TestMem,
        host_sends: &[u8],
    ) -> Vec<u8> {
        let slot = self.agent_rx_posted;
        let buf = 0x4007_0000 + u64::from(slot) * 0x100;
        post_rx(mem, AGENT_RX.desc, AGENT_RX.avail, buf, 64, slot);
        self.agent_rx_posted += 1;
        dev.agent_send(host_sends);
        dev.poll(mem);
        self.sync(mem);
        let used_idx = u16::from_le_bytes(mem.read(AGENT_RX_USED + 2, 2).try_into().unwrap());
        if used_idx != self.agent_rx_posted {
            return Vec::new();
        }
        let entry = AGENT_RX_USED + 4 + u64::from(slot) * 8;
        let len = u32::from_le_bytes(mem.read(entry + 4, 4).try_into().unwrap()) as usize;
        mem.read(buf, len)
    }
}

#[test]
fn a_killed_agent_that_reopens_the_port_hears_and_is_heard_without_a_new_handshake() {
    let mut dev = VirtioPciConsole::new();
    let mut mem = TestMem::new(0x4000_0000, 0x100000);
    setup_ready_queue_sized(&mut dev, &mut mem, 2, CONTROL_RX, CONTROL_RX_USED, 2, SIZE);
    setup_ready_queue_sized(&mut dev, &mut mem, 3, CONTROL_TX, 0x4000_6000, 3, SIZE);
    setup_ready_queue_sized(&mut dev, &mut mem, 4, AGENT_RX, AGENT_RX_USED, 4, SIZE);
    setup_ready_queue_sized(&mut dev, &mut mem, 5, AGENT_TX, 0x4000_c000, 5, SIZE);
    for n in 0..24u16 {
        let data = 0x4004_0000 + u64::from(n) * 0x100;
        post_control_rx(&mut mem, CONTROL_RX.desc, CONTROL_RX.avail, data, SIZE, n);
    }
    let mut guest = Guest {
        model: GuestModel::default(),
        control_seen: 0,
        control_tx: 0,
        agent_tx: 0,
        agent_rx_posted: 0,
    };

    // Boot: the only PORT_READY of this run.
    guest.control(
        &mut dev,
        &mut mem,
        0xffff_ffff,
        VIRTIO_CONSOLE_DEVICE_READY,
        1,
    );
    guest.control(&mut dev, &mut mem, 0, VIRTIO_CONSOLE_PORT_READY, 1);
    guest.control(
        &mut dev,
        &mut mem,
        AGENT_PORT_ID,
        VIRTIO_CONSOLE_PORT_READY,
        1,
    );

    // The first agent opens the port, says READY and gets a PING.
    guest.control(
        &mut dev,
        &mut mem,
        AGENT_PORT_ID,
        VIRTIO_CONSOLE_PORT_OPEN,
        1,
    );
    assert!(guest.write(&mut dev, &mut mem, b"READY first\n"));
    assert_eq!(dev.take_inbound(), b"READY first\n");
    assert!(dev.stats().agent_connected_confirmed);
    assert_eq!(guest.read(&mut dev, &mut mem, b"PING\n"), b"PING\n");

    // The agent is killed (vioser closes its handle), then its task starts a new one.
    guest.control(
        &mut dev,
        &mut mem,
        AGENT_PORT_ID,
        VIRTIO_CONSOLE_PORT_OPEN,
        0,
    );
    assert!(!dev.stats().port1_guest_open);
    guest.control(
        &mut dev,
        &mut mem,
        AGENT_PORT_ID,
        VIRTIO_CONSOLE_PORT_OPEN,
        1,
    );

    assert!(
        guest.write(&mut dev, &mut mem, b"READY second\n"),
        "the restarted agent's write must not block on HostConnected"
    );
    assert_eq!(
        dev.take_inbound(),
        b"READY second\n",
        "the host must hear the new READY"
    );
    assert_eq!(
        guest.read(&mut dev, &mut mem, b"PING\n"),
        b"PING\n",
        "the restarted agent must receive the host's next request"
    );
}
