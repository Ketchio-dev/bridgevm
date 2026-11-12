use super::*;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

fn shared() -> Shared {
    Shared {
        queue: -1,
        pending: AtomicBool::new(false),
        stop: AtomicBool::new(false),
        ready: Mutex::new(Vec::new()),
        deadline: Mutex::new(None),
    }
}

#[test]
fn one_socket_event_kicks_once_until_a_poll_takes_it() {
    let shared = shared();
    let (kicks, waits) = (AtomicUsize::new(0), AtomicUsize::new(0));
    std::thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            // One event, then nothing: later waits idle until the test stops the loop.
            let wait = || {
                let first = waits.fetch_add(1, SeqCst) == 0;
                if !first {
                    std::thread::sleep(Duration::from_millis(1));
                }
                first
            };
            run(&shared, wait, || {
                kicks.fetch_add(1, SeqCst);
            });
        });
        let started = Instant::now();
        while kicks.load(SeqCst) == 0 && started.elapsed() < Duration::from_secs(1) {
            std::thread::yield_now();
        }
        // The poll that answers the kick, as `HostWake::polled` does it. A
        // runner that runs this late sees re-kicks first, which is intended.
        assert!(
            shared.pending.swap(false, SeqCst),
            "no kick pending after the event"
        );
        let answered = kicks.load(SeqCst);
        watcher.thread().unpark();
        std::thread::sleep(REKICK * 3);
        shared.stop.store(true, SeqCst);
        watcher.thread().unpark();
        // At most one kick was already under way when the poll took it.
        assert!(
            kicks.load(SeqCst) <= answered + 1,
            "kicks went on after a poll answered"
        );
    });
}

#[test]
fn a_kick_no_poll_answers_is_sent_again() {
    let shared = shared();
    let kicks = AtomicUsize::new(0);
    // Timed from before the watcher starts until it has stopped, so every
    // counted kick falls inside the measured time.
    let started = Instant::now();
    std::thread::scope(|scope| {
        let watcher = scope.spawn(|| {
            let mut first = true;
            run(
                &shared,
                || std::mem::take(&mut first),
                || {
                    kicks.fetch_add(1, SeqCst);
                },
            );
        });
        // Wait for the re-kick rather than a fixed time: a loaded runner ran
        // the watcher late enough that 5 periods saw only the first kick.
        while kicks.load(SeqCst) < 2 && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(REKICK);
        }
        shared.stop.store(true, SeqCst);
        watcher.thread().unpark();
        watcher.join().unwrap();
    });
    let elapsed = started.elapsed();
    let kicks = kicks.load(SeqCst);
    assert!(kicks >= 2, "no re-kick in {elapsed:?}");
    // Paced by REKICK, not a spin.
    let periods = (elapsed.as_millis() / REKICK.as_millis()) as usize;
    assert!(kicks <= periods + 3, "{kicks} kicks in {elapsed:?}");
}

#[cfg(target_os = "macos")]
#[test]
fn nat_timer_wakes_for_an_unanswered_forwarded_syn() {
    idle_tcp_timer_wakes(0);
}

#[cfg(target_os = "macos")]
#[test]
fn nat_timer_wakes_for_unacknowledged_data() {
    idle_tcp_timer_wakes(1);
}

#[cfg(target_os = "macos")]
#[test]
fn nat_timer_wakes_for_a_zero_window_probe() {
    idle_tcp_timer_wakes(2);
}

#[cfg(target_os = "macos")]
fn idle_tcp_timer_wakes(kind: u8) {
    use crate::net_nat::*;
    use std::collections::VecDeque;
    use std::io::Write;
    use std::net::{Ipv4Addr, TcpListener, TcpStream};

    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
    let stream = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut peer, _) = listener.accept().unwrap();
    stream.set_nonblocking(true).unwrap();
    let mut handler = HostSocketOutboundIpv4Handler::with_dns_resolver(Ipv4Addr::LOCALHOST);
    handler
        .wake
        .set(Some(HostWake::start_unhooked().unwrap()))
        .unwrap();
    let key = TcpFlowKey {
        guest_ip: [10, 0, 2, 15],
        guest_port: 49163,
        dst_ip: [127, 0, 0, 1],
        dst_port: listener.local_addr().unwrap().port(),
    };
    let mut flow = TcpFlow::new(stream, 100, 200, if kind == 2 { 0 } else { 65535 }, None, 0);
    flow.state = if kind == 0 {
        TcpProxyState::Opening
    } else {
        TcpProxyState::Established
    };
    // No socket is registered: only a TCP deadline can wake this thread.
    flow.watched = true;
    if kind == 0 {
        flow.syn_sent(handler.now_ms());
    } else if kind == 1 {
        peer.write_all(b"unacknowledged").unwrap();
    }
    handler.tcp_flows.insert(key, flow);
    let mut replies = VecDeque::new();
    let mut stats = NatStats::default();
    let mac = Some([2, 0, 0, 0, 0, 1]);
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        // With no registered socket, explicitly poll until the initial bytes
        // arrive. After that only the TCP deadline can request another drain.
        handler.tcp_flows.get_mut(&key).unwrap().readable = true;
        handler.poll_host_sockets(mac, &mut replies, &mut stats);
        if kind != 1 || !replies.is_empty() {
            break;
        }
        assert!(Instant::now() < deadline, "host data never arrived");
        std::thread::yield_now();
    }
    replies.clear();
    let wake = handler.wake.get().unwrap().as_ref().unwrap();
    while !wake.shared.pending.load(SeqCst) && Instant::now() < deadline {
        std::thread::yield_now();
    }
    assert!(
        wake.shared.pending.load(SeqCst),
        "TCP timer did not wake an idle NAT (kind={kind})"
    );
    handler.poll_host_sockets(mac, &mut replies, &mut stats);
    assert_eq!(
        replies.len(),
        1,
        "deadline kick did not deliver the TCP timer"
    );
    if kind == 1 {
        assert_eq!(stats.tcp_timeout_retransmits, 1);
    }
    if kind == 2 {
        assert_eq!(stats.tcp_zero_window_probes, 1);
    }
}

#[cfg(target_os = "macos")]
#[test]
fn a_watched_socket_that_becomes_readable_marks_a_kick_pending() {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    let wake = imp::start().expect("kqueue");
    let (mut peer, ours) = std::os::unix::net::UnixStream::pair().unwrap();
    wake.watch(ours.as_raw_fd(), false);
    // Up to `wait` for a kick to become pending: 200 ms to show none comes,
    // 5 s for one that must (a loaded runner may run the thread late).
    let pending = |wait: Duration| {
        let started = Instant::now();
        while started.elapsed() < wait {
            if wake.shared.pending.load(SeqCst) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    };
    let (none, some) = (Duration::from_millis(200), Duration::from_secs(5));
    assert!(
        !pending(none),
        "a socket with nothing to read woke the thread"
    );
    peer.write_all(b"x").unwrap();
    assert!(
        pending(some),
        "data on a watched socket did not wake the thread"
    );
    wake.polled();
    // Edge-triggered: the unread byte alone does not wake it again.
    assert!(!pending(none), "the same data woke the thread twice");
    peer.write_all(b"y").unwrap();
    assert!(pending(some), "new data did not wake the thread again");
}
