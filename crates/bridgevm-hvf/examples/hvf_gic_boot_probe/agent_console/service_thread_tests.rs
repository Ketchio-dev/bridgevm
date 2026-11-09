//! vCPU0's exchange with the agent service thread: order, the bound, wakes, shutdown.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use std::sync::mpsc::{channel, Receiver};

const WAIT: Duration = Duration::from_secs(10);

/// The device as `exchange` sees it: guest chains leave the ring while its
/// buffer is under the bound (process_agent_tx_queue), and a send polls.
#[derive(Default)]
struct Wire {
    ring: VecDeque<Vec<u8>>,
    inbound: Vec<u8>,
    sent: Vec<u8>,
}

impl Wire {
    fn guest_writes(&mut self, bytes: &[u8]) {
        self.ring.push_back(bytes.to_vec());
        self.poll();
    }
}

impl AgentWire for Wire {
    fn drain_inbound_into(&mut self, out: &mut Vec<u8>) {
        out.append(&mut self.inbound);
    }
    fn poll(&mut self) {
        while self.inbound.len() < MAX_HOST_INBOUND_LEN {
            let Some(chain) = self.ring.pop_front() else {
                break;
            };
            self.inbound.extend_from_slice(&chain);
        }
    }
    fn send(&mut self, bytes: &[u8]) {
        self.sent.extend_from_slice(bytes);
        self.poll();
    }
    fn dns_server_changes(&self) -> Option<u64> {
        None
    }
}

fn harness_in(state: AgentConsoleState) -> AgentConsoleHarness {
    let mut h = harness();
    h.timeout = Duration::from_secs(3600);
    h.state = state;
    h
}

fn spawn(h: AgentConsoleHarness) -> (ServiceThread, Receiver<()>) {
    let (woke, wakes) = channel();
    let service = ServiceThread::spawn(h, move || {
        let _ = woke.send(());
    });
    (service, wakes)
}

#[test]
fn bytes_keep_their_order_both_ways_over_many_rounds() {
    let commands: Vec<String> = (0..16).map(|i| format!("echo {i}")).collect();
    let mut h = harness_in(AgentConsoleState::WaitingReady);
    h.commands = commands.clone();
    // The service waits in its wake until vCPU0 has exchanged twice, so each
    // reply reaches `rx` in two drains before the service reads it. (It sleeps:
    // a blocking receive would park the thread and use up vCPU0's unpark.)
    let (woke, wakes) = channel();
    let go = Arc::new(AtomicBool::new(false));
    let gate = Arc::clone(&go);
    let mut service = ServiceThread::spawn(h, move || {
        let _ = woke.send(());
        let deadline = Instant::now() + WAIT;
        while !gate.swap(false, Ordering::AcqRel) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    });
    let mut wire = Wire::default();
    let mut expected = b"PING\n".to_vec();
    let mut reply = b"PONG\n".to_vec();
    for (i, command) in commands.iter().enumerate() {
        wakes.recv_timeout(WAIT).expect("the service queued nothing");
        let (head, tail) = reply.split_at(2);
        wire.guest_writes(head);
        service.exchange_with(&mut wire);
        assert_eq!(String::from_utf8_lossy(&wire.sent), String::from_utf8_lossy(&expected));
        wire.guest_writes(tail);
        service.exchange_with(&mut wire);
        go.store(true, Ordering::Release);
        // Sent only once the harness read the whole reply.
        expected.extend_from_slice(command_wire_line(command).as_bytes());
        reply = format!("OK {i}\n").into_bytes();
    }
    wakes.recv_timeout(WAIT).expect("the service queued nothing");
    service.exchange_with(&mut wire);
    assert_eq!(String::from_utf8_lossy(&wire.sent), String::from_utf8_lossy(&expected));
    go.store(true, Ordering::Release);
}

#[test]
fn a_capped_drain_takes_the_chains_left_in_the_ring_and_rx_holds_at_the_bound() {
    let chain = vec![b'x'; MAX_HOST_INBOUND_LEN / 4];
    let mut wire = Wire::default();
    for _ in 0..5 {
        wire.guest_writes(&chain);
    }
    // The device is full and one chain waits in the ring.
    assert_eq!((wire.inbound.len(), wire.ring.len()), (MAX_HOST_INBOUND_LEN, 1));
    // A finished harness leaves `rx` alone, so only vCPU0's side moves bytes.
    let (mut service, _wakes) = spawn(harness_in(AgentConsoleState::Done));
    service.exchange_with(&mut wire);
    assert!(wire.ring.is_empty(), "a chain stayed in the ring after a capped drain");
    assert_eq!(wire.inbound.len(), chain.len());
    // `rx` is at the bound: the device keeps the rest, which holds the guest back.
    service.exchange_with(&mut wire);
    assert_eq!(wire.inbound.len(), chain.len());
    assert_eq!(lock(&service.shared.rx).len(), MAX_HOST_INBOUND_LEN);
}

#[test]
fn drop_returns_once_the_thread_and_its_harness_are_gone() {
    let (pasteboard, mut worker) = HostPasteboard::test_pair();
    // Owes a PING at once, so the thread is soon inside its (slow) wake.
    let mut h = harness_in(AgentConsoleState::WaitingReady);
    h.pasteboard = Some(pasteboard);
    let (entered, in_wake) = channel();
    let service = ServiceThread::spawn(h, move || {
        let _ = entered.send(());
        std::thread::sleep(Duration::from_millis(200));
    });
    in_wake.recv_timeout(WAIT).expect("the service never woke vCPU0");
    assert!(worker.report("before".into()), "the harness is alive while its thread runs");
    drop(service);
    assert!(!worker.report("after".into()), "Drop returned before the harness was dropped");
}

#[test]
fn vcpu0_is_woken_for_tx_readiness_and_a_full_rx_but_not_for_an_idle_step() {
    // Idle: a step with nothing for vCPU0 does not wake it, or each exchange
    // would unpark the service and each step kick vCPU0 again.
    let (mut service, wakes) = spawn(harness_in(AgentConsoleState::TimedOut));
    let mut wire = Wire::default();
    for _ in 0..3 {
        service.exchange_with(&mut wire);
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(wakes.recv_timeout(Duration::from_millis(100)).is_err(), "an idle step woke vCPU0");

    // TX: the PING owed while waiting for READY.
    let (mut service, wakes) = spawn(harness_in(AgentConsoleState::WaitingReady));
    wakes.recv_timeout(WAIT).expect("queued TX did not wake vCPU0");
    service.exchange_with(&mut wire);
    assert_eq!(wire.sent, b"PING\n");

    // Readiness without TX, then a full `rx` taken: the device may hold more.
    let (mut service, wakes) = spawn(harness_in(AgentConsoleState::WaitingOut { index: 0 }));
    wakes.recv_timeout(WAIT).expect("readiness did not wake vCPU0");
    assert!(service.desktop_ready());
    let mut wire = Wire::default();
    wire.guest_writes(&vec![b'x'; MAX_HOST_INBOUND_LEN]);
    service.exchange_with(&mut wire);
    wakes.recv_timeout(WAIT).expect("taking a full rx did not wake vCPU0");
    assert!(wire.sent.is_empty());
}
