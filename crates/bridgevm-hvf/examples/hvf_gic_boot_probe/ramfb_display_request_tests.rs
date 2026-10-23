//! The app's display tick asks the ramfb exporter for a frame through the
//! display file's header (display-export-20261023, F1 B2): the exporter
//! publishes at once, at most one frame per request, and exports on its
//! deadline only while nobody asks.

use std::fs::OpenOptions;
use std::os::unix::fs::FileExt;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::super::tests::{cleanup, config, frame, read_fb, scratch_path};
use super::super::{timing, RamfbDisplayThread};
use super::{exporter_with, published, wait_for, SharedConfig, SharedRam};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::fwcfg::GuestMemoryMut;

/// Longer than any test: only requests can publish after the first frame.
const NO_DEADLINE: Duration = Duration::from_secs(60);
/// The writer's mark that it answers requests, "BREQ" at bytes 40..44.
const MARKER: u32 = 0x5145_5242;

/// Guest RAM whose frame changes on every read, so every tick would publish.
struct RedrawnRam(Arc<AtomicU64>);

impl GuestMemoryMut for RedrawnRam {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, _gpa: u64, len: usize) -> Option<Vec<u8>> {
        let redraw = self.0.fetch_add(1, Ordering::SeqCst);
        Some(vec![redraw as u8; len])
    }
}

/// An exporter over `RedrawnRam`; the counter is the number of ticks.
fn redrawn_exporter(path: &Path, interval: Duration) -> (RamfbDisplayThread, Arc<AtomicU64>) {
    let ticks = Arc::new(AtomicU64::new(0));
    let (ram, source) = (
        RedrawnRam(Arc::clone(&ticks)),
        SharedConfig(Arc::new(Mutex::new((Some(config()), 0)))),
    );
    let target = DisplayFramebuffer::at_path(path);
    (
        RamfbDisplayThread::spawn(target, interval, source, move || ram),
        ticks,
    )
}

fn uptime_ns() -> u64 {
    let mut now = libc::timespec {
        tv_sec: 0,
        tv_nsec: 0,
    };
    unsafe { libc::clock_gettime(libc::CLOCK_UPTIME_RAW, &mut now) };
    now.tv_sec as u64 * 1_000_000_000 + now.tv_nsec as u64
}

/// What the app's display tick does: `pwrite` its `CLOCK_UPTIME_RAW`
/// nanoseconds into bytes 32..40 of the header.
fn request(path: &Path) {
    let file = OpenOptions::new().write(true).open(path).unwrap();
    file.write_all_at(&uptime_ns().to_le_bytes(), 32).unwrap();
}

fn marker(path: &Path) -> u32 {
    let file = std::fs::read(path).unwrap_or_default();
    file.get(40..44)
        .map_or(0, |word| u32::from_le_bytes(word.try_into().unwrap()))
}

/// The published sequence once no frame is being written.
fn settled(path: &Path) -> Option<u64> {
    let len = std::fs::metadata(path).ok()?.len();
    let sequence = (len >= 64).then(|| read_fb(path).1)?;
    (sequence & 1 == 0).then_some(sequence)
}

fn field(record: &str, name: &str) -> u64 {
    let value = record.split(' ').find_map(|field| field.strip_prefix(name));
    value
        .and_then(|value| value.parse().ok())
        .unwrap_or_else(|| panic!("{name} in {record}"))
}

#[test]
fn publishes_within_one_ms_of_a_frame_request() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    let (thread, _config) = exporter_with(&path, NO_DEADLINE, &ram, Some(config()));
    wait_for("the first frame", || {
        published(&path) == Some((2, frame(1)))
    });
    wait_for("the writer to answer requests", || marker(&path) == MARKER);
    let mut latencies = Vec::new();
    for redraw in 0..20u8 {
        ram.draw(&frame(redraw + 10));
        let before = settled(&path).unwrap();
        let posted = Instant::now();
        request(&path);
        while settled(&path).is_none_or(|sequence| sequence == before) {
            assert!(
                posted.elapsed() < Duration::from_secs(1),
                "request {redraw} unanswered"
            );
            std::thread::yield_now();
        }
        latencies.push(posted.elapsed());
        assert_eq!(published(&path), Some((before + 2, frame(redraw + 10))));
    }
    latencies.sort();
    let median = latencies[latencies.len() / 2];
    assert!(
        median <= Duration::from_millis(1),
        "median {median:?} of {latencies:?}"
    );
    // The record keeps its grammar: one tick per request after the first
    // frame, each late by the time since the app stamped its request.
    let record = timing::lock(&thread.timing).record();
    assert_eq!(
        (field(&record, "ticks="), field(&record, "published=")),
        (21, 21)
    );
    assert!(field(&record, "late_le_1ms=") >= 11, "{record}");
    drop(thread);
    cleanup(&path);
}

#[test]
fn publishes_at_most_one_frame_per_request() {
    let path = scratch_path();
    let (thread, ticks) = redrawn_exporter(&path, NO_DEADLINE);
    wait_for("the first frame", || settled(&path) == Some(2));
    wait_for("the writer to answer requests", || marker(&path) == MARKER);
    for answered in 1..=5 {
        request(&path);
        wait_for("the requested frame", || {
            settled(&path) == Some(2 + 2 * answered)
        });
    }
    // Three requests before the thread looks again: one to three frames.
    let before = settled(&path).unwrap();
    for _ in 0..3 {
        request(&path);
    }
    wait_for("the burst's frame", || settled(&path) > Some(before));
    std::thread::sleep(Duration::from_millis(50));
    let after = settled(&path).unwrap();
    assert!(
        (before + 2..=before + 6).contains(&after),
        "{before} -> {after}"
    );
    // Every read gives a new frame, yet without a request nothing is copied.
    std::thread::sleep(Duration::from_millis(100));
    assert_eq!(settled(&path), Some(after));
    assert_eq!(
        ticks.load(Ordering::SeqCst),
        after / 2,
        "one copy per published frame"
    );
    drop(thread);
    cleanup(&path);
}

#[test]
fn an_app_that_never_asks_still_gets_frames_on_the_deadline() {
    // An older app reads the file but never posts a request.
    let path = scratch_path();
    let (thread, ticks) = redrawn_exporter(&path, Duration::from_millis(20));
    wait_for("the writer to answer requests", || marker(&path) == MARKER);
    wait_for("deadline frames", || {
        settled(&path).is_some_and(|sequence| sequence >= 8)
    });
    assert!(ticks.load(Ordering::SeqCst) >= 4);
    drop(thread);
    cleanup(&path);
}

#[test]
fn the_deadline_exports_only_while_no_request_comes() {
    let path = scratch_path();
    let (thread, ticks) = redrawn_exporter(&path, Duration::from_millis(100));
    wait_for("the first frame", || {
        settled(&path).is_some_and(|sequence| sequence >= 2)
    });
    wait_for("the writer to answer requests", || marker(&path) == MARKER);
    // 40 requests at least 10 ms apart: a deadline on its own grid would add
    // at least 4 ticks; re-armed by each request it adds at most the one that
    // may race the first request.
    let before = ticks.load(Ordering::SeqCst);
    for _ in 0..40 {
        request(&path);
        std::thread::sleep(Duration::from_millis(10));
    }
    let during = ticks.load(Ordering::SeqCst) - before;
    assert!(during <= 41, "{during} ticks for 40 requests");
    // Nobody asks any more: the deadline exports again.
    let quiet = ticks.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(450));
    assert!(
        ticks.load(Ordering::SeqCst) - quiet >= 2,
        "the deadline stopped"
    );
    drop(thread);
    cleanup(&path);
}
