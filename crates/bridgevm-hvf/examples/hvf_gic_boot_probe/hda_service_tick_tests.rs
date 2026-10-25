//! The tick's thread, lock discipline and lifetime: it parks without a running
//! stream, never waits for or holds the platform lock while it sends, never
//! asks a vCPU to exit, spans reboot generations, and is joined before guest
//! RAM is unmapped.

use super::fixture::{ack, guest_ram, hda_platform, program, run_stream, Guest, HDA_MSI};
use super::fixture::{BYTES_PER_MS, PERIOD, RUN_IOCE};
use super::*;
use bridgevm_hvf::dtb::VirtFdtConfig;
use bridgevm_hvf::hda::REG_SD_CTL;
use bridgevm_hvf::platform_virt::FlatGuestRam;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::atomic::AtomicU64;
use std::sync::mpsc;

/// What a port on the shared machine saw, read while the tick thread owns it.
#[derive(Default)]
struct Seen {
    attempts: AtomicU64,
    /// Lock attempts made while the stream was stopped.
    stopped_attempts: AtomicU64,
    sends: AtomicU64,
    /// Sends that found the platform locked.
    locked_sends: AtomicU64,
    released: AtomicBool,
}

impl Seen {
    fn attempts(&self) -> u64 {
        self.attempts.load(SeqCst)
    }

    /// Wait until no tick has tried the lock for a while: the thread parked.
    fn settled(&self) -> u64 {
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            let before = self.attempts();
            thread::sleep(TICK_INTERVAL * 10);
            if self.attempts() == before {
                return before;
            }
            assert!(Instant::now() < deadline, "the tick never parked");
        }
    }
}

/// The tick thread's port on the shared machine, with its own view of guest
/// RAM; dropping it, as the thread ends, is the join the probe relies on.
struct Watched {
    platform: Arc<Mutex<VirtPlatform>>,
    mem: FlatGuestRam,
    activity: Arc<HdaStreamActivity>,
    seen: Arc<Seen>,
}

impl TickPort for Watched {
    fn service(&mut self, now: Instant) -> Service {
        self.seen.attempts.fetch_add(1, SeqCst);
        if !self.activity.running() {
            self.seen.stopped_attempts.fetch_add(1, SeqCst);
        }
        service_once(&self.platform, &PlatformHolds::new(), &mut self.mem, now)
    }

    fn send(&mut self, message: MsixMessage) -> HvReturn {
        assert_eq!(message, HDA_MSI);
        self.seen.sends.fetch_add(1, SeqCst);
        // From the tick's own thread: this fails if the tick still held it.
        match self.platform.try_lock() {
            Ok(mut platform) => ack(&mut platform, &mut self.mem),
            Err(_) => {
                self.seen.locked_sends.fetch_add(1, SeqCst);
            }
        }
        0
    }
}

impl Drop for Watched {
    fn drop(&mut self) {
        self.seen.released.store(true, SeqCst);
    }
}

fn watched_tick(platform: &Arc<Mutex<VirtPlatform>>) -> (HdaServiceTick, Arc<Seen>) {
    let seen = Arc::new(Seen::default());
    let activity = platform.lock().unwrap().hda_stream_activity().unwrap();
    let (owner, watched) = (Arc::clone(platform), Arc::clone(&seen));
    let tick = HdaServiceTick::start_with(platform, false, TICK_INTERVAL, move || Watched {
        platform: owner,
        mem: guest_ram(),
        activity,
        seen: watched,
    });
    assert!(tick.handle.is_some());
    (tick, seen)
}

fn wait_for(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(10);
    while !done() {
        assert!(Instant::now() < deadline, "timed out waiting for {what}");
        thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn hda_tick_skips_when_platform_is_locked() {
    let start = Instant::now();
    let mut guest = Guest::new(start);
    guest.write(REG_SD_CTL, 1, RUN_IOCE);
    let (gate, mut counts) = (Mutex::new(()), TickCounts::default());
    tick_once(&mut guest, &gate, start, &mut counts).unwrap();
    // A vCPU holds the platform while the next tick comes due.
    let (vcpu, (held_tx, held_rx)) = (Arc::clone(&guest.platform), mpsc::channel());
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let holder = thread::spawn(move || {
        let _platform = vcpu.lock().unwrap();
        held_tx.send(()).unwrap();
        release_rx.recv().unwrap();
    });
    held_rx.recv().unwrap();
    let (done_tx, done_rx) = mpsc::channel();
    let ticker = thread::spawn(move || {
        let result = tick_once(&mut guest, &gate, start + TICK_INTERVAL, &mut counts);
        done_tx.send(()).unwrap();
        (guest, gate, counts, result)
    });
    let returned = done_rx.recv_timeout(Duration::from_secs(10));
    release_tx.send(()).unwrap();
    holder.join().unwrap();
    returned.expect("the tick waited for the vCPU's lock");
    let (mut guest, gate, mut counts, result) = ticker.join().unwrap();
    assert_eq!(result, Ok(()));
    assert_eq!(
        (counts.ticks, counts.busy, guest.played.load(SeqCst)),
        (2, 1, 0)
    );
    // The next tick plays the whole 10 ms since the time base: nothing lost.
    tick_once(&mut guest, &gate, start + TICK_INTERVAL * 2, &mut counts).unwrap();
    assert_eq!(guest.played.load(SeqCst), 10 * BYTES_PER_MS);
    assert_eq!(guest.msis.len(), 1, "the completed period raised its MSI");
}

#[test]
fn hda_tick_is_parked_without_running_stream() {
    let (platform, _) = hda_platform();
    let (tick, seen) = watched_tick(&platform);
    thread::sleep(TICK_INTERVAL * 20);
    assert_eq!(seen.attempts(), 0, "no lock attempt before RUN");
    run_stream(&platform, true);
    wait_for("ticks while the stream runs", || seen.attempts() >= 5);
    run_stream(&platform, false);
    let parked = seen.settled();
    thread::sleep(TICK_INTERVAL * 20);
    assert_eq!(seen.attempts(), parked, "parked again once RUN cleared");
    let stopped = seen.stopped_attempts.load(SeqCst);
    assert!(
        stopped <= 1,
        "only a tick already past its check: {stopped}"
    );
    drop(tick);
}

#[test]
fn hda_tick_never_requests_vcpu_exit() {
    // Under the userspace GIC an MSI send is a vCPU kick: no tick at all.
    let (platform, _) = hda_platform();
    let never = || -> Guest { unreachable!("no port without a tick") };
    let tick = HdaServiceTick::start_with(&platform, true, TICK_INTERVAL, never);
    assert!(tick.handle.is_none());
    let plain = Mutex::new(VirtPlatform::new(VirtFdtConfig::default()));
    let tick = HdaServiceTick::start_with(&plain, false, TICK_INTERVAL, never);
    assert!(tick.handle.is_none(), "no HDA, nothing to service");
    // No exit call and no vCPU handle; the GIC is reached by one MSI send.
    let source = include_str!("hda_service_tick.rs");
    for absent in [
        "hv_vcpus_exit(",
        "HvVcpuT",
        "usgic_bridge::deliver",
        "hv_gic_set_spi",
    ] {
        assert!(!source.contains(absent), "{absent}");
    }
    assert_eq!(source.matches("deliver_pending_msix(").count(), 1);
    // That send is the port's provider: each IOC's message, and a failure
    // ends the tick (msix_platform_tests.rs's provider pattern).
    let (platform, _) = hda_platform();
    let (tick, seen) = watched_tick(&platform);
    run_stream(&platform, true);
    wait_for("MSIs through the provider", || seen.sends.load(SeqCst) >= 3);
    drop(tick);
    let start = Instant::now();
    let mut guest = Guest::new(start);
    guest.write(REG_SD_CTL, 1, RUN_IOCE);
    struct Failing(Guest);
    impl TickPort for Failing {
        fn service(&mut self, now: Instant) -> Service {
            self.0.service(now)
        }
        fn send(&mut self, _message: MsixMessage) -> HvReturn {
            0xfae9_4001_u32 as HvReturn
        }
    }
    let (gate, mut counts, mut failing) = (Mutex::new(()), TickCounts::default(), Failing(guest));
    let failed = Err(Halt::SendFailed(0xfae9_4001_u32 as HvReturn));
    for (tick, expected) in [Ok(()), failed].into_iter().enumerate() {
        let now = start + PERIOD * tick as u32;
        assert_eq!(tick_once(&mut failing, &gate, now, &mut counts), expected);
    }
}

#[test]
fn hda_tick_lifecycle_spans_reboot_generations_and_joins_before_unmap() {
    let (platform, _) = hda_platform();
    let activity = platform.lock().unwrap().hda_stream_activity().unwrap();
    let (tick, seen) = watched_tick(&platform);
    run_stream(&platform, true);
    wait_for("generation 1 ticks", || seen.attempts() >= 3);
    // SYSTEM_RESET: the reboot arm quiesces the tick across the resets.
    {
        let _quiesced = tick.quiesce();
        let frozen = seen.attempts();
        thread::sleep(TICK_INTERVAL * 10);
        assert_eq!(seen.attempts(), frozen, "quiesced: no poll, no MSI");
        platform.lock().unwrap().reset().unwrap();
    }
    assert!(!activity.running());
    let parked = seen.settled();
    // Generation 2's guest programs and runs the stream: the same tick wakes.
    program(&mut platform.lock().unwrap());
    run_stream(&platform, true);
    wait_for("generation 2 ticks", || seen.attempts() >= parked + 3);
    let now = platform.lock().unwrap().hda_stream_activity().unwrap();
    assert!(Arc::ptr_eq(&activity, &now));
    drop(tick);
    assert!(
        seen.released.load(SeqCst),
        "the drop joined it and its RAM view"
    );
    let after = seen.attempts();
    thread::sleep(TICK_INTERVAL * 10);
    assert_eq!(
        seen.attempts(),
        after,
        "joined: no poll with the stream running"
    );
    assert!(activity.running());
}

#[test]
fn unwinding_joins_the_tick() {
    let (platform, _) = hda_platform();
    let (seen, ticked) = (Arc::new(Mutex::new(None)), AtomicBool::new(false));
    let unwound = catch_unwind(AssertUnwindSafe(|| {
        let (_tick, watched) = watched_tick(&platform);
        *seen.lock().unwrap() = Some(Arc::clone(&watched));
        run_stream(&platform, true);
        wait_for("ticks", || watched.attempts() >= 2);
        ticked.store(true, SeqCst);
        panic!("the vCPU0 run loop panicked");
    }));
    assert!(
        unwound.is_err() && ticked.load(SeqCst),
        "it ticked, then unwound"
    );
    let seen = seen.lock().unwrap().take().unwrap();
    assert!(seen.released.load(SeqCst), "unwinding joined the tick");
    let after = seen.attempts();
    thread::sleep(TICK_INTERVAL * 10);
    assert_eq!(seen.attempts(), after);
}

#[test]
fn a_quiesced_tick_neither_polls_nor_sends_and_loses_no_time() {
    let start = Instant::now();
    let mut guest = Guest::new(start);
    guest.write(REG_SD_CTL, 1, RUN_IOCE);
    let (gate, mut counts) = (Mutex::new(()), TickCounts::default());
    tick_once(&mut guest, &gate, start, &mut counts).unwrap();
    {
        let _reset = gate.lock().unwrap();
        for tick in 1..=4 {
            let now = start + TICK_INTERVAL * tick;
            tick_once(&mut guest, &gate, now, &mut counts).unwrap();
        }
    }
    assert_eq!((counts.ticks, counts.quiesced), (1, 4));
    assert_eq!((guest.played.load(SeqCst), guest.msis.len()), (0, 0));
    tick_once(&mut guest, &gate, start + TICK_INTERVAL * 5, &mut counts).unwrap();
    assert_eq!(guest.played.load(SeqCst), 25 * BYTES_PER_MS);
    assert_eq!(guest.msis.len(), 1);
}

#[test]
fn the_probe_runs_one_tick_and_joins_it_before_the_report() {
    let run = include_str!("probe_runtime.rs");
    let start = "let hda_service_tick = HdaServiceTick::start(&platform, ram, ram_size);";
    assert_eq!(run.matches("HdaServiceTick::start(").count(), 1);
    let at = |needle: &str| run.find(needle).unwrap_or_else(|| panic!("{needle}"));
    assert!(at("GuestRamBacking::allocate_and_map") < at(start));
    assert!(at("RamfbDisplayThread::start(") < at(start));
    assert!(at(start) < at("'reboot: loop"), "once per process");
    let terminal = &run[run.rfind("continue 'reboot;").unwrap()..];
    let dropped = terminal.find("drop(hda_service_tick);").unwrap();
    assert!(dropped < terminal.find("persist_and_report_stop!(").unwrap());
    assert_eq!(run.matches("drop(hda_service_tick);").count(), 1);
    let reboot = &run[at("SystemResetDecision::Reboot {")..];
    let quiesced = reboot.find("= hda_service_tick.quiesce();").unwrap();
    assert!(quiesced < reboot.find("hv_gic_reset()").unwrap());
    assert!(quiesced < reboot.find("system_reset_failure(").unwrap());
}

#[test]
fn the_tick_never_holds_the_platform_lock_while_sending() {
    let (platform, _) = hda_platform();
    let (tick, seen) = watched_tick(&platform);
    run_stream(&platform, true);
    wait_for("several MSIs", || seen.sends.load(SeqCst) >= 5);
    drop(tick);
    assert_eq!(
        seen.locked_sends.load(SeqCst),
        0,
        "a send found the lock held"
    );
}

#[test]
fn vcpu_exits_and_the_tick_never_deadlock_or_duplicate_msis() {
    let (platform, _) = hda_platform();
    let (tick, seen) = watched_tick(&platform);
    run_stream(&platform, true);
    let (exit_msis, (done_tx, done_rx)) = (Arc::new(AtomicU64::new(0)), mpsc::channel());
    let vcpus: Vec<_> = (0..2u64)
        .map(|cpu| {
            let (platform, exit_msis, done) = (
                Arc::clone(&platform),
                Arc::clone(&exit_msis),
                done_tx.clone(),
            );
            thread::spawn(move || {
                let (mut mem, until) = (guest_ram(), Instant::now() + Duration::from_millis(300));
                while Instant::now() < until {
                    let mut platform = platform.lock().unwrap();
                    platform.set_host_now(Instant::now());
                    platform.poll_hda(&mut mem);
                    let taken = platform.take_pending_msix().len() as u64;
                    exit_msis.fetch_add(taken, SeqCst);
                    ack(&mut platform, &mut mem);
                    // A long platform section now and then, as an MMIO exit has.
                    let hold = Instant::now() + Duration::from_micros(200 * (cpu + 1));
                    while Instant::now() < hold {}
                    drop(platform);
                    thread::sleep(Duration::from_micros(300));
                }
                done.send(()).unwrap();
            })
        })
        .collect();
    for _ in &vcpus {
        done_rx
            .recv_timeout(Duration::from_secs(30))
            .expect("a vCPU deadlocked");
    }
    let (joined_tx, joined_rx) = mpsc::channel();
    thread::spawn(move || {
        drop(tick);
        joined_tx.send(()).unwrap();
    });
    joined_rx
        .recv_timeout(Duration::from_secs(30))
        .expect("the tick never joined");
    for vcpu in vcpus {
        vcpu.join().unwrap();
    }
    assert!(seen.attempts() > 0);
    // Every MSI the controller emitted reached exactly one sender.
    let emitted = platform.lock().unwrap().hda_diagnostic_records()[0].clone();
    let sent = exit_msis.load(SeqCst) + seen.sends.load(SeqCst);
    assert!(
        emitted.ends_with(&format!(" msis_sent={sent}")),
        "{emitted} vs {sent}"
    );
}
