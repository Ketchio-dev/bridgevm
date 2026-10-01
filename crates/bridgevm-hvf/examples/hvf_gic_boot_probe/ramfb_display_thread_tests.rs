use std::os::unix::fs::MetadataExt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::super::tests::{
    cleanup, config, frame, gpu_platform, read_fb, scratch_path, FB_ADDR, RAM_BASE,
};
use super::{ConfigSource, PlatformConfig, RamfbDisplayThread};
use bridgevm_hvf::display_fb::DisplayFramebuffer;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::VirtPlatform;
use bridgevm_hvf::ramfb::RamfbConfig;

const TICK: Duration = Duration::from_millis(5);

/// Guest RAM the test can redraw while the exporter thread reads it.
#[derive(Clone, Default)]
struct SharedRam {
    state: Arc<Mutex<RamState>>,
    fail: Arc<AtomicBool>,
    released: Arc<AtomicBool>,
}

/// The exporter thread's handle on `SharedRam`. The thread drops it on its way
/// out, so it must be gone by the time dropping the exporter returns: that is
/// the join the probe relies on before it unmaps guest RAM.
struct HeldRam(SharedRam);

impl Drop for HeldRam {
    fn drop(&mut self) {
        self.0.released.store(true, Ordering::SeqCst);
    }
}

impl GuestMemoryMut for HeldRam {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        self.0.read_bytes(gpa, len)
    }
}

#[derive(Default)]
struct RamState {
    bytes: Vec<u8>,
    reads: Vec<Instant>,
    reader: Option<String>,
}

impl SharedRam {
    fn with(pixels: &[u8]) -> Self {
        let ram = Self::default();
        ram.state.lock().unwrap().bytes = vec![0; 0x1000];
        ram.draw(pixels);
        ram
    }

    fn draw(&self, pixels: &[u8]) {
        let at = (FB_ADDR - RAM_BASE) as usize;
        self.state.lock().unwrap().bytes[at..at + pixels.len()].copy_from_slice(pixels);
    }

    fn reads(&self) -> Vec<Instant> {
        self.state.lock().unwrap().reads.clone()
    }

    fn released(&self) -> bool {
        self.released.swap(false, Ordering::SeqCst)
    }
}

impl GuestMemoryMut for SharedRam {
    fn write_bytes(&mut self, _gpa: u64, _data: &[u8]) -> bool {
        false
    }

    fn read_bytes(&self, gpa: u64, len: usize) -> Option<Vec<u8>> {
        assert!(!self.fail.load(Ordering::SeqCst), "guest RAM read failed");
        let mut state = self.state.lock().unwrap();
        state.reads.push(Instant::now());
        state.reader = std::thread::current().name().map(str::to_owned);
        let start = usize::try_from(gpa.checked_sub(RAM_BASE)?).ok()?;
        state.bytes.get(start..start.checked_add(len)?).map(<[u8]>::to_vec)
    }
}

/// The ramfb config as the test sets it, and how many times it was polled.
#[derive(Clone)]
struct SharedConfig(Arc<Mutex<(Option<RamfbConfig>, u64)>>);

impl SharedConfig {
    /// Set the config and wait out any tick that polled the previous one.
    fn set(&self, config: Option<RamfbConfig>) {
        let polled = {
            let mut state = self.0.lock().unwrap();
            state.0 = config;
            state.1
        };
        wait_for("two more polls", || self.0.lock().unwrap().1 >= polled + 2);
    }
}

impl ConfigSource for SharedConfig {
    fn poll(&mut self) -> Option<RamfbConfig> {
        let mut state = self.0.lock().unwrap();
        state.1 += 1;
        state.0
    }
}

fn exporter(path: &Path, interval: Duration, ram: &SharedRam) -> RamfbDisplayThread {
    exporter_with(path, interval, ram, Some(config())).0
}

fn exporter_with(
    path: &Path,
    interval: Duration,
    ram: &SharedRam,
    config: Option<RamfbConfig>,
) -> (RamfbDisplayThread, SharedConfig) {
    let (source, ram) = (SharedConfig(Arc::new(Mutex::new((config, 0)))), ram.clone());
    let target = DisplayFramebuffer::at_path(path);
    let thread = RamfbDisplayThread::spawn(target, interval, source.clone(), || HeldRam(ram));
    (thread, source)
}

/// The published (header, sequence, pixels) once the file holds a frame.
fn fb(path: &Path) -> Option<(Vec<u32>, u64, Vec<u8>)> {
    let len = std::fs::metadata(path).ok()?.len();
    (len >= 64).then(|| read_fb(path))
}

fn published(path: &Path) -> Option<(u64, Vec<u8>)> {
    fb(path).map(|(_, sequence, pixels)| (sequence, pixels))
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        std::thread::sleep(Duration::from_millis(1));
    }
}

/// A memory factory that records whether an exporter thread ever ran.
fn watched_memory(ran: &Arc<AtomicBool>) -> impl FnOnce() -> SharedRam + Send + 'static {
    let ran = Arc::clone(ran);
    move || {
        ran.store(true, Ordering::SeqCst);
        SharedRam::with(&frame(1))
    }
}

#[test]
fn publishes_changed_frames_from_its_own_thread_and_is_silent_once_dropped() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    let first = exporter(&path, TICK, &ram);
    wait_for("the first frame", || published(&path) == Some((2, frame(1))));
    let reader = ram.state.lock().unwrap().reader.clone();
    assert_eq!(reader.as_deref(), Some("ramfb-display-export"), "not a vCPU");
    ram.draw(&frame(9));
    wait_for("the redrawn frame", || published(&path) == Some((4, frame(9))));
    drop(first);
    assert!(ram.released(), "dropping the exporter joined its thread");
    let reads = ram.reads().len();
    ram.draw(&frame(5));
    std::thread::sleep(TICK * 10);
    assert_eq!(ram.reads().len(), reads, "joined: nothing reads guest RAM");
    assert_eq!(published(&path), Some((4, frame(9))), "and nothing writes");
    // Process recreation: the next writer continues in place.
    let inode = std::fs::metadata(&path).unwrap().ino();
    let second = exporter(&path, TICK, &ram);
    wait_for("the next writer", || published(&path) == Some((6, frame(5))));
    assert_eq!(std::fs::metadata(&path).unwrap().ino(), inode);
    drop(second);
    cleanup(&path);
}

#[test]
fn unchanged_frames_are_polled_but_only_changes_are_published() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    let (thread, source) = exporter_with(&path, TICK, &ram, Some(config()));
    wait_for("the first frame", || published(&path).is_some());
    let reads = ram.reads().len();
    wait_for("five more polls", || ram.reads().len() >= reads + 5);
    assert_eq!(read_fb(&path).1, 2, "identical frames are not republished");
    let narrower = RamfbConfig {
        width: 2,
        ..config()
    };
    source.set(Some(narrower));
    wait_for("the new geometry", || {
        fb(&path).is_some_and(|(header, sequence, _)| (header[2], sequence) == (2, 4))
    });
    source.set(None);
    let reads = ram.reads().len();
    std::thread::sleep(TICK * 5);
    assert_eq!(ram.reads().len(), reads, "inactive ramfb is not read");
    drop(thread);
    assert_eq!(read_fb(&path).1, 4);
    cleanup(&path);
}

#[test]
fn never_polls_faster_than_its_period() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    let interval = Duration::from_millis(20);
    let spawned = Instant::now();
    let thread = exporter(&path, interval, &ram);
    wait_for("six polls", || ram.reads().len() >= 6);
    drop(thread);
    for (tick, read) in ram.reads().into_iter().enumerate() {
        let earliest = spawned + interval * u32::try_from(tick).unwrap();
        assert!(read >= earliest, "poll {tick} ran ahead of its period");
    }
    cleanup(&path);
}

#[test]
fn a_stop_wakes_the_thread_without_waiting_out_its_period() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    let thread = exporter(&path, Duration::from_secs(60), &ram);
    wait_for("the first frame", || published(&path).is_some());
    let stopping = Instant::now();
    drop(thread);
    assert!(ram.released() && stopping.elapsed() < Duration::from_secs(5));
    cleanup(&path);
}

#[test]
fn inert_without_a_display_path() {
    let platform = Arc::new(Mutex::new(VirtPlatform::new(VirtFdtConfig::default())));
    let ran = Arc::new(AtomicBool::new(false));
    let thread = RamfbDisplayThread::start_with(&platform, None, TICK, watched_memory(&ran));
    assert!(thread.handle.is_none());
    std::thread::sleep(TICK * 4);
    assert!(!ran.load(Ordering::SeqCst));
}

#[test]
fn inert_for_the_process_when_the_machine_has_a_virtio_gpu_device() {
    let path = scratch_path();
    let target = || Some(DisplayFramebuffer::at_path(&path));
    let gpu = Arc::new(Mutex::new(gpu_platform()));
    let ran = Arc::new(AtomicBool::new(false));
    let thread = RamfbDisplayThread::start_with(&gpu, target(), TICK, watched_memory(&ran));
    assert!(thread.handle.is_none(), "the virtio-gpu device owns the file");
    std::thread::sleep(TICK * 4);
    assert!(!ran.load(Ordering::SeqCst) && !path.exists());
    // The same call on a machine without the device does start the writer.
    let plain = Arc::new(Mutex::new(VirtPlatform::new(VirtFdtConfig::default())));
    let thread = RamfbDisplayThread::start_with(&plain, target(), TICK, watched_memory(&ran));
    assert!(thread.handle.is_some());
    wait_for("the plain machine's writer", || ran.load(Ordering::SeqCst));
    drop(thread);
    assert!(!path.exists(), "no ramfb programmed, nothing published");
    cleanup(&path);
}

#[test]
fn a_platform_poll_never_waits_for_a_lock_holder_and_fails_closed() {
    let platform = Arc::new(Mutex::new(VirtPlatform::new(VirtFdtConfig::default())));
    let mut source = PlatformConfig {
        platform: Arc::clone(&platform),
        last: Some(config()),
    };
    {
        let _vcpu = platform.lock().unwrap();
        assert_eq!(source.poll(), Some(config()), "held: the last value, at once");
    }
    assert_eq!(source.poll(), None, "free: the platform's own (inactive) config");
    source.last = Some(config());
    let poisoner = Arc::clone(&platform);
    let _ = std::thread::spawn(move || {
        let _vcpu = poisoner.lock().unwrap();
        panic!("a vCPU panicked holding the platform");
    })
    .join();
    assert_eq!(source.poll(), None);
    let path = scratch_path();
    let ran = Arc::new(AtomicBool::new(false));
    let target = Some(DisplayFramebuffer::at_path(&path));
    let thread = RamfbDisplayThread::start_with(&platform, target, TICK, watched_memory(&ran));
    assert!(thread.handle.is_none(), "ownership unknown: no writer");
}

#[test]
fn unwinding_the_owning_thread_joins_the_exporter() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let _exporter = exporter(&path, TICK, &ram);
        wait_for("the first frame", || published(&path).is_some());
        panic!("the vCPU0 run loop panicked");
    }));
    assert!(unwound.is_err());
    assert!(ram.released(), "unwinding joined the exporter");
    let reads = ram.reads().len();
    std::thread::sleep(TICK * 5);
    assert_eq!(ram.reads().len(), reads, "the exporter was joined, not leaked");
    cleanup(&path);
}

#[test]
fn a_panicking_exporter_neither_hangs_nor_propagates() {
    let path = scratch_path();
    let ram = SharedRam::with(&frame(1));
    ram.fail.store(true, Ordering::SeqCst);
    let thread = exporter(&path, TICK, &ram);
    wait_for("the thread to end", || {
        thread.handle.as_ref().is_some_and(|handle| handle.is_finished())
    });
    drop(thread);
    assert!(!path.exists());
}

#[test]
fn the_vcpu0_run_loop_no_longer_exports_ramfb_frames() {
    let run = include_str!("probe_runtime.rs");
    for gone in ["RamfbDisplayExporter", "ramfb_display.due", "ramfb_display.export"] {
        assert!(!run.contains(gone), "vCPU0 still references {gone}");
    }
    let start = "let ramfb_display = RamfbDisplayThread::start(";
    assert_eq!(run.matches("RamfbDisplayThread::start(").count(), 1);
    let at = |needle: &str| run.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(at("GuestRamBacking::allocate_and_map") < at(start));
    assert!(at(start) < at("'reboot: loop"), "once per process");
}
