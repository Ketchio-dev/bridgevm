use super::{HostCounters, PauseGate, PauseOutcome, Resumed};
use std::cell::Cell;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;

/// Stops and releases the fake vCPUs even when an assertion fails, so a
/// failure cannot hang the thread scope.
struct Release<'a>(&'a PauseGate, &'a AtomicBool);

impl Drop for Release<'_> {
    fn drop(&mut self) {
        self.1.store(true, Ordering::SeqCst);
        let _ = self.0.resume(&Ticks(0));
    }
}

/// Three fake secondaries: each passes the gate, then "runs" (counts).
#[test]
fn pause_parks_every_vcpu_and_resume_shifts_one_offset_for_all() {
    let gate = PauseGate::new();
    let stop = AtomicBool::new(false);
    let runs: [AtomicU64; 3] = Default::default();
    let applied: Mutex<Vec<u64>> = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for runs in &runs {
            let (gate, stop, applied) = (&gate, &stop, &applied);
            scope.spawn(move || {
                let _in_loop = gate.enter_run_loop();
                let mut version = 0;
                while !stop.load(Ordering::SeqCst) {
                    gate.before_run(&mut version, |offset| {
                        applied.lock().unwrap().push(offset);
                        offset
                    });
                    runs.fetch_add(1, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(1));
                }
            });
        }
        let _release = Release(&gate, &stop);
        while runs.iter().any(|count| count.load(Ordering::SeqCst) == 0) {
            std::thread::yield_now();
        }

        assert!(matches!(gate.pause(|| {}, &Ticks(1_000)), PauseOutcome::Paused));
        let frozen: Vec<u64> = runs.iter().map(|count| count.load(Ordering::SeqCst)).collect();
        std::thread::sleep(Duration::from_millis(50));
        let later: Vec<u64> = runs.iter().map(|count| count.load(Ordering::SeqCst)).collect();
        assert_eq!(frozen, later, "no vCPU runs while paused");
        assert!(applied.lock().unwrap().is_empty());

        // 24 MHz counter: 60 s paused.
        assert_eq!(gate.resume(&Ticks(1_000 + 1_440_000_000)), Some(Resumed { paused: 1_440_000_000, offset: 1_440_000_000, slept: 0 }));
        while runs.iter().zip(&later).any(|(count, at)| count.load(Ordering::SeqCst) == *at) {
            std::thread::yield_now();
        }
        stop.store(true, Ordering::SeqCst);
    });
    assert_eq!(*applied.lock().unwrap(), vec![1_440_000_000; 3], "each vCPU took the offset once");
    assert_eq!(gate.resume(&Ticks(0)), None, "not paused");
}

/// A second pause adds to the first; vCPU0 and a vCPU created afterwards take the same offset.
#[test]
fn offsets_accumulate_and_later_vcpus_take_the_same_value() {
    let gate = PauseGate::new();
    for (pause_at, resume_at) in [(100, 600), (1_000, 1_250)] {
        assert!(matches!(gate.pause(|| {}, &Ticks(pause_at)), PauseOutcome::Paused));
        assert!(gate.is_paused());
        gate.resume(&Ticks(resume_at)).unwrap();
    }
    let mut cpu0 = 0;
    let mut set = Vec::new();
    gate.take_offset(&mut cpu0, |offset| {
        set.push(offset);
        offset
    });
    gate.take_offset(&mut cpu0, |offset| {
        set.push(offset);
        offset
    });
    let mut created_later = 0;
    gate.before_run(&mut created_later, |offset| {
        set.push(offset);
        offset
    });
    assert_eq!(set, vec![750, 750], "once per vCPU, the same value");
}

/// A secondary stuck outside the gate must not hang vCPU0: the pause is abandoned.
#[test]
fn a_vcpu_that_never_parks_abandons_the_pause() {
    let gate = PauseGate::new();
    let _stuck = gate.enter_run_loop();
    let kicked = AtomicBool::new(false);
    let outcome = gate.pause(|| kicked.store(true, Ordering::SeqCst), &Ticks(7));
    assert!(matches!(outcome, PauseOutcome::Abandoned));
    assert!(kicked.load(Ordering::SeqCst));
    assert!(!gate.is_paused());
    let mut version = 0;
    gate.before_run(&mut version, |_| panic!("no offset change"));
}

/// The paused wait wakes on an input.ctl append, not only on its timeout.
#[test]
fn input_wakes_the_paused_wait() {
    let gate = PauseGate::new();
    assert!(matches!(gate.pause(|| {}, &Ticks(0)), PauseOutcome::Paused));
    std::thread::scope(|scope| {
        scope.spawn(|| {
            std::thread::sleep(Duration::from_millis(20));
            gate.notify_input();
        });
        let started = std::time::Instant::now();
        gate.wait_for_input(Duration::from_secs(10));
        assert!(started.elapsed() < Duration::from_secs(5));
    });
}

/// Both host counters at one value: no sleep.
struct Ticks(u64);

impl HostCounters for Ticks {
    fn mach(&self) -> u64 {
        self.0
    }

    fn raw(&self) -> u64 {
        self.0
    }
}

/// The raw counter runs ahead of mach time by what the Mac slept.
struct Counters {
    mach: Cell<u64>,
    raw: Cell<u64>,
}

impl HostCounters for Counters {
    fn mach(&self) -> u64 {
        self.mach.get()
    }

    fn raw(&self) -> u64 {
        self.raw.get()
    }
}

/// Review on 3d9f317f: HVF's guest counter is mach time minus the offset. A
/// 60 s pause during which the Mac slept 30 s shifts the offset by the mach
/// ticks only, so the guest counter continues where it stopped; measured on the
/// raw counter, the offset would grow 30 s too far and the counter go backwards.
#[test]
fn a_pause_across_a_mac_sleep_shifts_the_offset_by_mach_time_only() {
    let gate = PauseGate::new();
    let counters = Counters { mach: Cell::new(5_000), raw: Cell::new(9_000) };
    assert!(matches!(gate.pause(|| {}, &counters), PauseOutcome::Paused));
    let guest_at_pause = counters.mach.get() - gate.offset();
    counters.mach.set(5_000 + 1_440_000_000);
    counters.raw.set(9_000 + 1_440_000_000 + 720_000_000);
    let resumed = gate.resume(&counters).unwrap();
    assert_eq!(resumed, Resumed { paused: 1_440_000_000, offset: 1_440_000_000, slept: 720_000_000 });
    assert_eq!(counters.mach.get() - gate.offset(), guest_at_pause, "the guest counter continues");
}

/// The emulated CNTPCT_EL0 follows the pause offset, and a restored
/// checkpoint's offset is the base a later pause adds to.
#[test]
fn the_physical_count_and_a_restored_offset_follow_the_pause_offset() {
    let gate = PauseGate::new();
    gate.seed_offset(1_000);
    assert_eq!(gate.guest_count(50_000), 49_000);
    assert!(matches!(gate.pause(|| {}, &Ticks(60_000)), PauseOutcome::Paused));
    assert_eq!(gate.resume(&Ticks(84_000)).unwrap().offset, 25_000, "added to the restored offset");
    assert_eq!(gate.guest_count(100_000), 75_000);
}

/// Every PAUSE and RESUME gets an answer, also when the VM already is what it asks.
#[test]
fn a_resume_for_a_running_vm_and_a_repeated_pause_are_answered() {
    use super::{request, Request};
    assert_eq!(request(Some(false), false), Request::AlreadyRunning);
    assert_eq!(request(Some(true), true), Request::AlreadyPaused);
    assert_eq!(request(Some(true), false), Request::Pause);
    assert_eq!(request(Some(false), true), Request::Resume);
    assert_eq!(request(None, false), Request::None);
    assert_eq!(super::resumed_line(0, 0x2b), "BVPAUSE resumed paused_ms=0 vtimer_offset=0x2b");
}

/// A pause for a full Mac disk says why first, in the line the app reads
/// (HvfGuestPause.swift): the errno, the bytes free and the volume.
#[test]
fn a_disk_full_pause_names_the_errno_free_space_and_volume() {
    let full = bridgevm_hvf::nvme::DiskFull {
        errno: libc::ENOSPC,
        free_bytes: 4096,
        volume: "/Volumes/Mac SSD".into(),
    };
    assert_eq!(
        super::storage_full_line(&full),
        "BVSTORAGE full errno=28 free=4096 path=/Volumes/Mac SSD"
    );
}

/// Review on #576 (P1): run.log can sit on the volume that just filled, and a
/// failed `println!` panics, which ended the VMM before the pause. Here the
/// disk-full report and the pause lines go to a stdout that refuses every
/// write (a pipe nobody reads, so EPIPE where a full volume gives ENOSPC;
/// macOS has no /dev/full), in a child process, which must carry on.
#[test]
fn the_disk_full_lines_survive_a_stdout_that_refuses_them() {
    const CHILD: &str = "BV_PAUSE_GATE_REFUSING_STDOUT";
    if std::env::var_os(CHILD).is_some() {
        let mut pipe = [0; 2];
        // SAFETY: plain descriptor calls on descriptors this test owns; fd 1
        // is put back before the test returns.
        let saved = unsafe {
            assert_eq!(libc::pipe(pipe.as_mut_ptr()), 0);
            libc::close(pipe[0]);
            let saved = libc::dup(1);
            assert_eq!(libc::dup2(pipe[1], 1), 1);
            saved
        };
        let full = bridgevm_hvf::nvme::DiskFull {
            errno: libc::ENOSPC,
            free_bytes: 0,
            volume: "/".into(),
        };
        super::report_storage_full(&full);
        super::answer_running();
        bridgevm_hvf::host_log::out("BVPAUSE paused");
        // SAFETY: as above.
        unsafe {
            libc::dup2(saved, 1);
            libc::close(saved);
            libc::close(pipe[1]);
        }
        return;
    }
    let module = module_path!().split_once("::").expect("crate::module").1;
    let name = format!("{module}::the_disk_full_lines_survive_a_stdout_that_refuses_them");
    let child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", &name, "--nocapture", "--test-threads=1"])
        .env(CHILD, "1")
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&child.stdout);
    assert!(stdout.contains("1 passed"), "{stdout}{}", String::from_utf8_lossy(&child.stderr));
}
