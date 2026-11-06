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
        // The poll that answers the kick, as `HostWake::polled` does it.
        assert!(
            shared.pending.swap(false, SeqCst),
            "no kick pending after the event"
        );
        watcher.thread().unpark();
        std::thread::sleep(REKICK * 3);
        shared.stop.store(true, SeqCst);
        watcher.thread().unpark();
    });
    assert_eq!(kicks.load(SeqCst), 1, "one event, one poll: one kick");
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
        std::thread::sleep(REKICK * 5);
        shared.stop.store(true, SeqCst);
        watcher.thread().unpark();
    });
    let kicks = kicks.load(SeqCst);
    assert!(
        (2..=8).contains(&kicks),
        "{kicks} kicks in 5 re-kick periods"
    );
}

#[cfg(target_os = "macos")]
#[test]
fn a_watched_socket_that_becomes_readable_marks_a_kick_pending() {
    use std::io::Write;
    use std::os::fd::AsRawFd;
    let wake = imp::start().expect("kqueue");
    let (mut peer, ours) = std::os::unix::net::UnixStream::pair().unwrap();
    wake.watch(ours.as_raw_fd(), false);
    let pending = || {
        let started = Instant::now();
        while started.elapsed() < Duration::from_millis(200) {
            if wake.shared.pending.load(SeqCst) {
                return true;
            }
            std::thread::sleep(Duration::from_millis(1));
        }
        false
    };
    assert!(!pending(), "a socket with nothing to read woke the thread");
    peer.write_all(b"x").unwrap();
    assert!(
        pending(),
        "data on a watched socket did not wake the thread"
    );
    wake.polled();
    // Edge-triggered: the unread byte alone does not wake it again.
    assert!(!pending(), "the same data woke the thread twice");
    peer.write_all(b"y").unwrap();
    assert!(pending(), "new data did not wake the thread again");
}
