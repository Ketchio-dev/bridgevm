//! The tick's per-tick work on synthetic host time, so schedules replay
//! exactly: the audio model's test (a), and T17 r92's exit bursts with and
//! without the tick. The thread only adds the waiting (hda_service_tick_tests.rs).

use super::fixture::{Guest, BYTES_PER_MS, PERIOD, RUN_IOCE};
use super::*;
use bridgevm_hvf::hda::REG_SD_CTL;

/// How late each simulated wake is, cycled; always under one interval.
const LATENESS_US: [u64; 5] = [0, 3000, 1000, 4000, 2000];

#[test]
fn hda_tick_advances_stream_without_vcpu_exits() {
    let start = Instant::now();
    let mut guest = Guest::new(start);
    // The RUN write; from here on no vCPU exit polls the controller.
    guest.write(REG_SD_CTL, 1, RUN_IOCE);
    let (gate, mut counts) = (Mutex::new(()), TickCounts::default());
    for tick in 0..=50u32 {
        let late = match tick {
            0 | 50 => 0,
            _ => LATENESS_US[tick as usize % LATENESS_US.len()],
        };
        let now = start + TICK_INTERVAL * tick + Duration::from_micros(late);
        tick_once(&mut guest, &gate, now, &mut counts).unwrap();
    }
    // 250 ms of DMA from the first tick's time base, to one frame.
    let played = guest.played.load(SeqCst);
    assert!(
        played.abs_diff(250 * BYTES_PER_MS) <= 4,
        "played {played} bytes"
    );
    // 25 periods: 25 MSIs, each at most one interval after its boundary.
    assert_eq!(guest.msis.len(), 25);
    for (index, at) in guest.msis.iter().enumerate() {
        let boundary = start + PERIOD * (index as u32 + 1);
        let delay = at.saturating_duration_since(boundary);
        assert!(
            *at >= boundary && delay <= TICK_INTERVAL,
            "MSI {index}: {delay:?}"
        );
    }
    let r = guest.records();
    let per_poll = [r["ioc_completions"], r["msis_sent"], r["max_iocs_per_poll"]];
    assert_eq!(per_poll, [25, 25, 1]);
    assert_eq!((counts.ticks, counts.msis, counts.busy), (51, 25, 0));
}

/// T17 r92's running-poll gaps beyond its exit bursts: 54 of 20-50 ms (here
/// 30), one of 50-100 ms (75) and the 216 ms maximum, the long ones mid-stream.
fn r92_gaps() -> Vec<Duration> {
    let mut gaps = vec![Duration::from_millis(30); 54];
    gaps.insert(18, Duration::from_millis(216));
    gaps.insert(37, Duration::from_millis(75));
    gaps
}

/// Replays the schedule from RUN: bursts of exits 0.1 ms apart around each
/// gap, and, with the tick, a deadline-paced tick 0.25 ms late every 5 ms.
fn replay(with_tick: bool) -> (Guest, TickCounts) {
    let start = Instant::now();
    let mut guest = Guest::new(start);
    guest.write(REG_SD_CTL, 1, RUN_IOCE);
    // The RUN write's own drain takes the time base.
    let (mut exits, mut at) = (vec![start], start);
    for gap in r92_gaps().into_iter().map(Some).chain([None]) {
        for _ in 0..100 {
            at += Duration::from_micros(100);
            exits.push(at);
        }
        if let Some(gap) = gap {
            at += gap;
            exits.push(at);
        }
    }
    let late = Duration::from_micros(250);
    let ticks: Vec<Instant> = (1..)
        .map(|tick| start + TICK_INTERVAL * tick + late)
        .take_while(|tick| with_tick && *tick <= at)
        .collect();
    let (gate, mut counts) = (Mutex::new(()), TickCounts::default());
    let (mut exits, mut ticks) = (exits.into_iter().peekable(), ticks.into_iter().peekable());
    // Events in host-time order; an exit first when both fall at once.
    loop {
        let exit_first = match (exits.peek(), ticks.peek()) {
            (Some(exit), Some(tick)) => exit <= tick,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };
        if exit_first {
            guest.exit_poll(exits.next().unwrap());
        } else {
            tick_once(&mut guest, &gate, ticks.next().unwrap(), &mut counts).unwrap();
        }
    }
    (guest, counts)
}

#[test]
fn hda_tick_bounds_gaps_under_live_burst_schedule() {
    // Exits alone give r92's buckets, so the replay has the run's shape.
    let (guest, counts) = replay(false);
    let r = guest.records();
    let buckets = [r["gap_le_50ms"], r["gap_le_100ms"], r["gap_gt_100ms"]];
    assert_eq!(buckets, [54, 1, 1], "{r:?}");
    assert_eq!([r["capped_polls"], r["capped_lost_ms"]], [1, 116]);
    assert!(r["repeat_passes"] > 0, "stale laps replayed: {r:?}");
    assert!(r["msis_sent"] < r["ioc_completions"], "IOCs shared MSIs");
    assert_eq!(counts, TickCounts::default());
    // With the tick no running poll waits past one interval of lateness:
    // nothing over 10 ms, nothing capped, no lap replayed, an MSI per IOC.
    let (guest, counts) = replay(true);
    let r = guest.records();
    let over_10ms = [
        r["gap_le_20ms"],
        r["gap_le_50ms"],
        r["gap_le_100ms"],
        r["gap_gt_100ms"],
    ];
    assert_eq!(over_10ms, [0; 4], "{r:?}");
    assert_eq!([r["capped_polls"], r["repeat_passes"]], [0, 0], "{r:?}");
    assert_eq!(
        r["compared_passes"],
        r["passes"] - 2,
        "all but the first laps: {r:?}"
    );
    assert_eq!(r["msis_sent"], r["ioc_completions"]);
    assert_eq!(r["max_iocs_per_poll"], 1);
    assert!(counts.msis > 0 && counts.ticks > 400, "{counts:?}");
}
