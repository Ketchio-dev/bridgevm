use super::*;

const MS: Duration = Duration::from_millis(1);

#[test]
fn a_quiet_queue_pauses_once_and_the_next_write_starts_it() {
    let t0 = Instant::now();
    let mut gate = IdleGate::new(0, t0);
    assert_eq!(gate.step(0, t0 + IDLE_PAUSE - MS), None, "not idle yet");
    assert_eq!(gate.sleep(t0 + IDLE_PAUSE - MS), Some(MS));
    assert_eq!(gate.step(0, t0 + IDLE_PAUSE), Some(Step::Pause));
    assert_eq!(gate.step(0, t0 + 10 * IDLE_PAUSE), None, "paused once");
    assert_eq!(gate.sleep(t0 + 10 * IDLE_PAUSE), None, "a write wakes it");
    assert_eq!(gate.step(1, t0 + 10 * IDLE_PAUSE), Some(Step::Start));
    assert_eq!(gate.step(1, t0 + 10 * IDLE_PAUSE + MS), None);
}

#[test]
fn every_write_restarts_the_quiet_period() {
    let t0 = Instant::now();
    let mut gate = IdleGate::new(0, t0);
    let mut writes = 0;
    for tick in 1..=20 {
        writes += 1;
        assert_eq!(
            gate.step(writes, t0 + tick * IDLE_PAUSE / 2),
            None,
            "a running stream"
        );
    }
    let last = t0 + 10 * IDLE_PAUSE;
    assert_eq!(gate.sleep(last), Some(IDLE_PAUSE));
    assert_eq!(gate.step(writes, last + IDLE_PAUSE - MS), None);
    assert_eq!(gate.step(writes, last + IDLE_PAUSE), Some(Step::Pause));
}

#[test]
fn a_write_racing_the_pause_is_seen_by_the_supervisor_or_wakes_it() {
    let control = OutputControl::default();
    control.register_supervisor(std::thread::current());
    // The supervisor decided to pause on this count...
    let seen = control.writes();
    // ...and a write lands before it publishes the pause: its next read sees it.
    control.note_pcm();
    control.set_paused(true);
    assert_ne!(control.writes(), seen);
    // A write after the pause leaves an unpark token, so park returns at once.
    control.note_pcm();
    let parked = Instant::now();
    std::thread::park_timeout(Duration::from_secs(5));
    assert!(
        parked.elapsed() < Duration::from_secs(4),
        "the write woke the supervisor"
    );
    assert_eq!(control.writes(), seen + 2);
}

#[test]
fn supervisor_lines_reach_the_producer_once_in_order_and_bounded() {
    let control = OutputControl::default();
    assert!(control.take_notices().is_empty(), "nothing waits");
    control.notice("first".into());
    control.notice("second".into());
    assert_eq!(control.take_notices(), ["first", "second"]);
    assert!(control.take_notices().is_empty(), "each line once");
    control.notice("later".into());
    assert_eq!(control.take_notices(), ["later"]);
    for n in 0..2 * MAX_NOTICES {
        control.notice(n.to_string());
    }
    assert_eq!(control.take_notices().len(), MAX_NOTICES);
}
