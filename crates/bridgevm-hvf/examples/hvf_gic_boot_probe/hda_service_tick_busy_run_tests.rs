//! `max_busy_run_ms` on the `hda service tick: stopped` line: the longest run
//! of consecutive ticks that found the platform busy, times the interval.

use super::*;
use std::collections::VecDeque;

/// A platform that answers each tick from a script.
struct Script(VecDeque<Service>);

impl TickPort for Script {
    fn service(&mut self, _now: Instant) -> Service {
        self.0.pop_front().expect("one scripted answer per tick")
    }

    fn send(&mut self, _message: MsixMessage) -> HvReturn {
        0
    }
}

fn busy(ticks: usize) -> impl Iterator<Item = Service> {
    (0..ticks).map(|_| Service::Busy)
}

fn polled() -> impl Iterator<Item = Service> {
    std::iter::once(Service::Polled(None))
}

#[test]
fn the_stopped_line_reports_the_longest_busy_run_in_ms() {
    let script = busy(3)
        .chain(polled())
        .chain(busy(7))
        .chain(polled())
        .chain(busy(2));
    let mut port = Script(script.collect());
    let (gate, mut counts) = (Mutex::new(()), TickCounts::default());
    let start = Instant::now();
    for tick in 0..14 {
        let now = start + TICK_INTERVAL * tick;
        tick_once(&mut port, &gate, now, &mut counts).unwrap();
    }
    assert_eq!(
        (counts.ticks, counts.busy, counts.max_busy_run),
        (14, 12, 7)
    );
    assert_eq!(
        counts.stopped_record(TICK_INTERVAL),
        "hda service tick: stopped streams=0 ticks=14 busy=12 quiesced=0 msis=0 \
         max_late_us=0 max_busy_run_ms=35"
    );
}

#[test]
fn a_quiesced_tick_or_a_new_stream_ends_a_busy_run() {
    let mut port = Script(busy(9).collect());
    let (gate, mut counts) = (Mutex::new(()), TickCounts::default());
    let now = Instant::now();
    let mut ticks = |counts: &mut TickCounts, n| {
        for _ in 0..n {
            tick_once(&mut port, &gate, now, counts).unwrap();
        }
    };
    ticks(&mut counts, 4);
    let reset = gate.lock().unwrap();
    let mut quiesced = Script(VecDeque::new());
    tick_once(&mut quiesced, &gate, now, &mut counts).unwrap();
    drop(reset);
    ticks(&mut counts, 3);
    counts.note_stream();
    ticks(&mut counts, 2);
    let seen = (
        counts.busy,
        counts.quiesced,
        counts.streams,
        counts.max_busy_run,
    );
    assert_eq!(seen, (9, 1, 1, 4), "{counts:?}");
}

#[test]
fn the_thread_prints_this_line_as_it_ends() {
    let tick = include_str!("hda_service_tick.rs");
    let print = "println!(\"{}\", c.stopped_record(interval));";
    assert_eq!(tick.matches(print).count(), 1);
    assert_eq!(tick.matches("c.stopped_record(").count(), 1);
}
