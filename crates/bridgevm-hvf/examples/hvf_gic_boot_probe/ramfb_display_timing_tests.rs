use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::super::tests::{cleanup, config, frame, gpu_platform, ram_with, scratch_path};
use super::super::{ConfigSource, RamfbDisplayThread};
use super::{lock, RamfbDisplayTiming};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::ramfb::RamfbConfig;

const TICK: Duration = Duration::from_millis(5);
const FIELDS: [&str; 13] = [
    "ticks",
    "late_le_1ms",
    "late_le_4ms",
    "late_le_8ms",
    "late_le_16ms",
    "late_gt_16ms",
    "max_late_us",
    "published",
    "interval_le_20ms",
    "interval_le_40ms",
    "interval_le_70ms",
    "interval_gt_70ms",
    "max_interval_ms",
];

/// The record's values, in order, after checking its prefix and field names.
fn values(record: &str) -> Vec<u64> {
    let fields = record
        .strip_prefix("ramfb display timing: ")
        .expect("the record's prefix");
    let pairs: Vec<_> = fields
        .split(' ')
        .map(|f| f.split_once('=').unwrap())
        .collect();
    assert_eq!(
        pairs.iter().map(|(name, _)| *name).collect::<Vec<_>>(),
        FIELDS
    );
    pairs
        .iter()
        .map(|(_, value)| value.parse().unwrap())
        .collect()
}

fn us(value: u64) -> Duration {
    Duration::from_micros(value)
}

#[test]
fn ticks_count_by_how_late_they_woke_against_their_deadline() {
    let (mut timing, base) = (RamfbDisplayTiming::default(), Instant::now());
    for late in [
        0, 400, 1_000, 1_500, 4_000, 7_900, 8_000, 12_000, 16_000, 16_001, 250_000,
    ] {
        timing.note_tick(base, base + us(late), None);
    }
    // The loop never wakes early; if it did, that tick would count as on time.
    timing.note_tick(base + us(5_000), base, None);
    let record = timing.record();
    assert_eq!(
        values(&record),
        [12, 4, 2, 2, 2, 2, 250_000, 0, 0, 0, 0, 0, 0]
    );
}

#[test]
fn published_frames_count_by_the_interval_since_the_previous_one() {
    let (mut timing, base) = (RamfbDisplayTiming::default(), Instant::now());
    let mut at = base;
    timing.note_tick(at, at, Some(at));
    for gap in [
        16_000, 20_000, 20_001, 40_000, 55_000, 70_000, 70_001, 1_500_000,
    ] {
        at += us(gap);
        timing.note_tick(at, at, Some(at));
    }
    // A tick without a new frame keeps the previous frame as the interval's start.
    timing.note_tick(at, at + us(3_000), None);
    at += us(30_000);
    timing.note_tick(at, at, Some(at));
    assert_eq!(
        timing.record(),
        "ramfb display timing: ticks=11 late_le_1ms=10 late_le_4ms=1 late_le_8ms=0 \
         late_le_16ms=0 late_gt_16ms=0 max_late_us=3000 published=10 interval_le_20ms=2 \
         interval_le_40ms=3 interval_le_70ms=2 interval_gt_70ms=2 max_interval_ms=1500"
    );
}

#[test]
fn the_widest_record_is_unsigned_decimals_that_fit_u64() {
    let mut timing = RamfbDisplayTiming {
        ticks: u64::MAX,
        late: [u64::MAX; 5],
        max_late: Duration::MAX,
        published: u64::MAX,
        intervals: [u64::MAX; 4],
        max_interval: Duration::MAX,
        last_published: None,
    };
    let base = Instant::now();
    timing.note_tick(base, base + us(20_000), Some(base));
    timing.note_tick(base, base, Some(base + us(80_000)));
    assert_eq!(values(&timing.record()), [u64::MAX; 13]);
}

/// A fixed ramfb geometry, as firmware leaves it programmed.
struct Programmed(Option<RamfbConfig>);

impl ConfigSource for Programmed {
    fn poll(&mut self) -> Option<RamfbConfig> {
        self.0
    }
}

/// Poll `records` until the exporter's record shows `ticks` ticks.
fn wait_for_ticks(ticks: u64, records: impl Fn() -> Vec<String>) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while records()
        .first()
        .map_or(true, |record| values(record)[0] < ticks)
    {
        assert!(
            Instant::now() < deadline,
            "the exporter never ticked {ticks} times"
        );
        std::thread::sleep(TICK);
    }
}

#[test]
fn the_export_loop_counts_its_ticks_and_published_frames() {
    let path = scratch_path();
    let target = DisplayFramebuffer::at_path(&path);
    let source = Programmed(Some(config()));
    let thread = RamfbDisplayThread::spawn(target, TICK, source, || ram_with(&frame(7)));
    let timing = Arc::clone(&thread.timing);
    wait_for_ticks(3, || vec![lock(&timing).record()]);
    drop(thread);
    let after = values(&lock(&timing).record());
    assert!(
        after[0] >= 3 && after[1..6].iter().sum::<u64>() == after[0],
        "{after:?}"
    );
    assert_eq!(after[7], 1, "one unchanged frame is published once");
    assert_eq!(
        after[8..12].iter().sum::<u64>(),
        0,
        "a single frame has no interval"
    );
    cleanup(&path);
}

#[test]
fn a_running_exporter_leaves_its_record_with_the_platform_and_an_inert_one_none() {
    let path = scratch_path();
    let target = || Some(DisplayFramebuffer::at_path(&path));
    let memory = || || ram_with(&frame(1));
    let plain = Arc::new(Mutex::new(VirtPlatform::new(VirtFdtConfig::default())));
    let records = || plain.lock().unwrap().teardown_host_records();
    assert!(records().is_empty());
    let thread = RamfbDisplayThread::start_with(&plain, target(), TICK, memory());
    wait_for_ticks(2, records);
    drop(thread);
    let joined = records();
    assert_eq!(joined.len(), 1);
    assert_eq!(
        values(&joined[0])[7],
        0,
        "no ramfb programmed, nothing published"
    );
    std::thread::sleep(TICK * 4);
    assert_eq!(
        records(),
        joined,
        "the joined exporter's final counts stay with the platform"
    );

    let gpu = Arc::new(Mutex::new(gpu_platform()));
    let thread = RamfbDisplayThread::start_with(&gpu, target(), TICK, memory());
    assert!(thread.handle.is_none());
    drop(thread);
    assert!(
        gpu.lock().unwrap().teardown_host_records().is_empty(),
        "virtio-gpu owns the file"
    );
    let headless = Arc::new(Mutex::new(VirtPlatform::new(VirtFdtConfig::default())));
    drop(RamfbDisplayThread::start_with(
        &headless,
        None,
        TICK,
        memory(),
    ));
    assert!(
        headless.lock().unwrap().teardown_host_records().is_empty(),
        "no display path"
    );
    cleanup(&path);
}
