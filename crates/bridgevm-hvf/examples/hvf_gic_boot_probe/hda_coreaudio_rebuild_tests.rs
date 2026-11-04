use super::*;

fn asked(watch: &Watch, signals: Signals, now: Instant, running: bool) -> (Option<Rebuild>, bool) {
    let mut asked = false;
    let decision = watch.rebuild(signals, now, || {
        asked = true;
        running
    });
    (decision, asked)
}

#[test]
fn a_default_output_change_rebuilds_a_playing_or_a_paused_queue() {
    let t0 = Instant::now();
    let changed = Signals {
        default_output_changed: true,
        ..Signals::default()
    };
    let mut watch = Watch::started(t0);
    assert_eq!(
        asked(&watch, changed, t0, true).0,
        Some(Rebuild::DefaultOutputChanged)
    );
    watch.note_paused();
    assert_eq!(
        asked(&watch, changed, t0, true).0,
        Some(Rebuild::DefaultOutputChanged)
    );
}

#[test]
fn a_failed_start_or_enqueue_rebuilds() {
    let t0 = Instant::now();
    let watch = Watch::started(t0);
    let start = Signals {
        start_failed: true,
        ..Signals::default()
    };
    let enqueue = Signals {
        enqueue_failed: true,
        ..Signals::default()
    };
    assert_eq!(asked(&watch, start, t0, true).0, Some(Rebuild::StartFailed));
    assert_eq!(
        asked(&watch, enqueue, t0, true).0,
        Some(Rebuild::EnqueueFailed)
    );
}

#[test]
fn a_stopped_queue_rebuilds_only_once_it_should_have_started() {
    let t0 = Instant::now();
    let mut watch = Watch::started(t0);
    let quiet = Signals::default();
    // Just started: a late device start is not a stop, and nobody asks.
    assert_eq!(
        asked(&watch, quiet, t0 + START_GRACE / 2, false),
        (None, false)
    );
    assert_eq!(asked(&watch, quiet, t0 + START_GRACE, true), (None, true));
    assert_eq!(
        asked(&watch, quiet, t0 + START_GRACE, false),
        (Some(Rebuild::StoppedRunning), true)
    );
    // Paused by the supervisor: not asked, whatever it would report.
    watch.note_paused();
    assert_eq!(
        asked(&watch, quiet, t0 + 10 * START_GRACE, false),
        (None, false)
    );
    watch.note_started(t0 + 10 * START_GRACE);
    assert_eq!(
        asked(&watch, quiet, t0 + 10 * START_GRACE, false),
        (None, false)
    );
}
