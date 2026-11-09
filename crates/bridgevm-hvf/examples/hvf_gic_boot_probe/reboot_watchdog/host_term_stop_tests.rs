use super::*;
use std::sync::atomic::AtomicUsize;
use std::sync::mpsc;
use std::time::Instant;

#[test]
fn before_term_the_watcher_sleeps_instead_of_polling() {
    // The real kqueue wait, and TERM never comes. A TERM another test raises
    // in this process can only end the waiting early.
    let (requested, run) = (AtomicBool::new(false), BootProgressWatchdog::new());
    let (term, waits) = (TermEvent::new(), AtomicUsize::new(0));
    let counted = || {
        waits.fetch_add(1, Ordering::SeqCst);
        term.wait()
    };
    std::thread::scope(|scope| {
        scope.spawn(|| watch(&requested, &run, counted, || {}));
        std::thread::sleep(Duration::from_millis(1100));
        let woke = waits.load(Ordering::SeqCst);
        run.disarm();
        // A 10 ms poll wakes about 110 times in this window.
        assert!(woke <= 3, "the watcher woke {woke} times in 1.1 s before TERM");
    });
}

#[test]
fn after_term_the_watcher_kicks_vcpu0_promptly_and_keeps_kicking() {
    // The watcher kicks at once on TERM and then every WATCH_INTERVAL (10 ms); one that missed
    // the TERM, or kicked once and went back to its wait, would take ARMED_CHECK (1 s). Half a
    // second tells the two apart and holds on a loaded CI runner (a 50 ms window did not).
    let prompt = ARMED_CHECK / 2;
    let (requested, run) = (AtomicBool::new(false), BootProgressWatchdog::new());
    let (term_tx, term_rx) = mpsc::channel::<()>();
    let (kick_tx, kick_rx) = mpsc::channel::<Instant>();
    // Stands in for the kqueue: the injected TERM ends the wait, as the
    // signal's event does.
    let wait = move || term_rx.recv_timeout(ARMED_CHECK).is_ok();
    let kick = move || kick_tx.send(Instant::now()).unwrap();
    std::thread::scope(|scope| {
        scope.spawn(|| watch(&requested, &run, wait, kick));
        std::thread::sleep(Duration::from_millis(100));
        let early = kick_rx.try_recv().is_ok();

        let sent = Instant::now();
        requested.store(true, Ordering::SeqCst);
        term_tx.send(()).unwrap();
        let first = kick_rx.recv_timeout(prompt);
        let second = kick_rx.recv_timeout(prompt);
        run.disarm(); // before any assert: the scope joins the watcher
        assert!(!early, "kicked before TERM");
        let first = first.expect("no prompt kick after TERM");
        assert!(first - sent < prompt);
        assert!(second.is_ok(), "one kick only; a kick lost between two runs would stall the stop");
    });
}
