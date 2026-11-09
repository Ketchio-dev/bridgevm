//! vCPU0's exchange with the agent service thread: order, the bound, wakes, shutdown.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;
use std::sync::mpsc::{channel, Receiver};

const WAIT: Duration = Duration::from_secs(10);
/// A backstop tick that never comes in a test: only an unpark steps the service.
const PARKED: Duration = Duration::from_secs(3600);
/// How long a service that should stay parked is watched.
const QUIET: Duration = Duration::from_millis(300);

/// The device as `exchange` sees it: guest chains leave the ring while its
/// buffer is under the bound (process_agent_tx_queue), and a send polls.
#[derive(Default)]
struct Wire {
    ring: VecDeque<Vec<u8>>,
    inbound: Vec<u8>,
    sent: Vec<u8>,
    dns: Option<u64>,
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
        self.dns
    }
}

fn harness_in(state: AgentConsoleState) -> AgentConsoleHarness {
    let mut h = harness();
    h.timeout = Duration::from_secs(3600);
    h.state = state;
    h
}

/// The device and the platform lock around it, as the service reaches them.
type Device = Arc<Mutex<Wire>>;

impl AgentDevice for Device {
    fn drain_into(&self, rx: &Mutex<Vec<u8>>) -> usize {
        let mut wire = lock(self);
        drain(rx, |out| wire.drain_inbound_into(out))
    }
}

fn spawn(h: AgentConsoleHarness) -> (ServiceThread, Receiver<()>) {
    spawn_ticking(h, &Arc::default(), SERVICE_TICK)
}

/// A service on its own device, which these tests leave empty: vCPU0's
/// exchange with a test's `Wire` drained the guest's bytes first.
fn spawn_ticking(
    h: AgentConsoleHarness,
    slot: &Arc<ServiceSlot>,
    tick: Duration,
) -> (ServiceThread, Receiver<()>) {
    spawn_on(h, &Device::default(), slot, tick)
}

fn spawn_on(
    h: AgentConsoleHarness,
    device: &Device,
    slot: &Arc<ServiceSlot>,
    tick: Duration,
) -> (ServiceThread, Receiver<()>) {
    let (woke, wakes) = channel();
    let service = ServiceThread::start(
        h,
        Arc::clone(device),
        slot,
        move || {
            let _ = woke.send(());
        },
        tick,
    );
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
    let wait_for_go = move || {
        let _ = woke.send(());
        let deadline = Instant::now() + WAIT;
        while !gate.swap(false, Ordering::AcqRel) && Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(1));
        }
    };
    let mut service =
        ServiceThread::start(h, Device::default(), &Arc::default(), wait_for_go, SERVICE_TICK);
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
    let slow_wake = move || {
        let _ = entered.send(());
        std::thread::sleep(Duration::from_millis(200));
    };
    let service =
        ServiceThread::start(h, Device::default(), &Arc::default(), slow_wake, SERVICE_TICK);
    in_wake.recv_timeout(WAIT).expect("the service never woke vCPU0");
    assert!(worker.report("before".into()), "the harness is alive while its thread runs");
    drop(service);
    assert!(!worker.report("after".into()), "Drop returned before the harness was dropped");
}

#[test]
fn vcpu0_is_woken_for_tx_readiness_and_a_full_rx_but_not_for_an_idle_step() {
    // Idle: a step with nothing for vCPU0 does not wake it, or each exchange
    // would unpark the service and each step kick vCPU0 again. Guest bytes
    // step the service; a finished harness leaves them.
    let (mut service, wakes) = spawn_ticking(
        harness_in(AgentConsoleState::TimedOut),
        &Arc::default(),
        PARKED,
    );
    let mut wire = Wire::default();
    for _ in 0..3 {
        wire.guest_writes(b"x");
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

#[test]
fn an_exchange_unparks_the_service_only_with_news_for_it() {
    // Each case starts a service that steps only when unparked; its harness
    // owes a PING, so a step wakes vCPU0.
    let after = |news: &dyn Fn(&mut ServiceThread, &mut Wire)| {
        let (mut service, wakes) = spawn_ticking(
            harness_in(AgentConsoleState::WaitingReady),
            &Arc::default(),
            PARKED,
        );
        let mut wire = Wire {
            dns: Some(0),
            ..Wire::default()
        };
        news(&mut service, &mut wire);
        service.exchange_with(&mut wire);
        (service, wakes)
    };
    // Automation ticks that moved nothing, with the DNS count unchanged: every
    // tick while the pointer moves.
    let (_service, wakes) = after(&|service, wire| {
        service.exchange_with(wire);
        service.exchange_with(wire);
    });
    assert!(wakes.recv_timeout(QUIET).is_err(), "a tick with no news stepped the service");

    let news: [(&str, &dyn Fn(&mut ServiceThread, &mut Wire)); 5] = [
        ("guest bytes", &|_, wire| wire.guest_writes(b"P")),
        ("a send", &|service, _| lock(&service.shared.tx).extend_from_slice(b"x")),
        ("a DNS switch", &|_, wire| wire.dns = Some(1)),
        ("a VM pause", &|service, _| service.note_vm_pause(Duration::from_secs(1))),
        // Last: the request stays pending on this thread.
        ("a desktop request", &|_, _| request_desktop_mode((1920, 1080))),
    ];
    for (name, news) in news {
        let (_service, wakes) = after(news);
        assert!(wakes.recv_timeout(WAIT).is_ok(), "{name} did not step the service");
    }
}

#[test]
fn a_ctl_append_wakes_the_service_not_vcpu0() {
    let path = std::env::temp_dir().join(format!(
        "bvagent-service-thread-ctl-{}.ctl",
        std::process::id()
    ));
    std::fs::write(&path, b"PING\n").unwrap();
    let now = Instant::now();
    let mut h = harness_in(AgentConsoleState::Service);
    h.service = true;
    h.ctl_path = Some(path.to_string_lossy().into_owned());
    h.last_send = Some(now);
    h.last_alive = Some(now);
    let slot = Arc::default();
    let (mut service, wakes) = spawn_ticking(h, &slot, PARKED);
    // What the ctl watcher thread does on each append; it holds only the slot.
    crate::agent_console::ctl_wake::ctl_changed(&slot);
    // The service read the command and woke vCPU0 to send it, with no exchange before.
    wakes.recv_timeout(WAIT).expect("the ctl append did not reach the service");
    let mut wire = Wire::default();
    service.exchange_with(&mut wire);
    assert_eq!(wire.sent, b"PING\n");
    drop(service);
    assert!(lock(&slot.0).is_none(), "the slot outlived its boot's service thread");
    std::fs::remove_file(path).unwrap();
}

#[test]
fn a_guest_reply_reaches_the_service_with_no_vcpu0_exit() {
    let device = Device::default();
    let slot = Arc::default();
    // No PING owed: the service sends only once it reads the reply.
    let mut h = harness_in(AgentConsoleState::WaitingReady);
    h.last_ping = Some(Instant::now());
    let (mut service, wakes) = spawn_on(h, &device, &slot, PARKED);
    // The notify exit put the reply in the device and ran the reply hook.
    lock(&device).guest_writes(b"READY BRIDGEVM v3\n");
    crate::agent_console::ctl_wake::guest_replied(&slot);
    // The service drained and read it with no vCPU0 exchange; its first wake
    // is for the PING it owes now, and READY.
    wakes.recv_timeout(WAIT).expect("the reply did not reach the service");
    assert!(service.desktop_ready());
    assert!(lock(&device).inbound.is_empty());
    let mut wire = lock(&device);
    service.exchange_with(&mut *wire);
    assert_eq!(wire.sent, b"PING\n");
}

#[test]
fn bytes_keep_their_order_when_vcpu0_and_the_service_both_drain() {
    let device = Device::default();
    let slot = Arc::default();
    // A finished harness leaves `rx` alone, so it shows every drained byte.
    let (mut service, _wakes) = spawn_on(harness_in(AgentConsoleState::Done), &device, &slot, PARKED);
    let mut expected = Vec::new();
    for i in 0..2000 {
        let bytes = format!("{i},");
        expected.extend_from_slice(bytes.as_bytes());
        let mut wire = lock(&device);
        wire.guest_writes(bytes.as_bytes());
        if i % 3 == 0 {
            service.exchange_with(&mut *wire);
        }
        drop(wire);
        crate::agent_console::ctl_wake::guest_replied(&slot);
    }
    service.exchange_with(&mut *lock(&device));
    assert!(lock(&device).inbound.is_empty());
    assert_eq!(
        String::from_utf8_lossy(&lock(&service.shared.rx)),
        String::from_utf8_lossy(&expected)
    );
}

#[test]
fn a_full_device_the_service_drained_wakes_vcpu0_to_poll_the_ring() {
    let device = Device::default();
    let chain = vec![b'x'; MAX_HOST_INBOUND_LEN / 4];
    for _ in 0..5 {
        lock(&device).guest_writes(&chain);
    }
    // The device is full, one chain waits in the ring, and no notify will come.
    // A timed-out harness leaves `rx` alone and is not ready: the drain's is
    // the only wake.
    let slot = Arc::default();
    let h = harness_in(AgentConsoleState::TimedOut);
    let (mut service, wakes) = spawn_on(h, &device, &slot, PARKED);
    slot.unpark();
    wakes.recv_timeout(WAIT).expect("a full drain did not wake vCPU0");
    let mut wire = lock(&device);
    assert_eq!((wire.inbound.len(), wire.ring.len()), (0, 1));
    // `rx` is full, so vCPU0 drains nothing, but it polls the ring.
    service.exchange_with(&mut *wire);
    assert!(wire.ring.is_empty(), "the chain stayed in the ring");
}
