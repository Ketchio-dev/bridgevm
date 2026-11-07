use super::*;
use std::sync::atomic::AtomicUsize;
use std::time::Instant;

fn shared() -> Shared {
    Shared {
        queue: -1,
        pending: AtomicBool::new(false),
        stop: AtomicBool::new(false),
        ready: Mutex::new(Vec::new()),
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
        let started = Instant::now();
        while kicks.load(SeqCst) < 2 && started.elapsed() < Duration::from_secs(5) {
            std::thread::sleep(REKICK);
        }
        let elapsed = started.elapsed();
        shared.stop.store(true, SeqCst);
        watcher.thread().unpark();
        let kicks = kicks.load(SeqCst);
        assert!(kicks >= 2, "no re-kick in {elapsed:?}");
        // Paced by REKICK, not a spin.
        let periods = (elapsed.as_millis() / REKICK.as_millis()) as usize;
        assert!(kicks <= periods + 3, "{kicks} kicks in {elapsed:?}");
    });
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
