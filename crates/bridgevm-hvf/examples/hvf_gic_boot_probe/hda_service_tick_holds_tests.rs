//! The tick's busy attempts reach the `platform lock holds:` record: a vCPU's
//! NVMe exit that keeps the lock across ticks is charged with every tick it
//! blocked, and the thread prints the record right after its stopped line.

use super::fixture::{guest_ram, hda_platform, run_stream};
use super::*;
use crate::platform_lock_holds::tests::{fields, map_nvme, NVME_BAR};
use crate::platform_lock_holds::{HoldSite, Holder};
use bridgevm_hvf::platform_virt::FlatGuestRam;
use std::sync::mpsc;

/// The probe's port with its own holds rather than the process's.
struct Charged<'a> {
    platform: Arc<Mutex<VirtPlatform>>,
    holds: &'a PlatformHolds,
    mem: FlatGuestRam,
}

impl TickPort for Charged<'_> {
    fn service(&mut self, now: Instant) -> Service {
        service_once(&self.platform, self.holds, &mut self.mem, now)
    }

    fn send(&mut self, _message: MsixMessage) -> HvReturn {
        0
    }
}

#[test]
fn an_nvme_exit_that_keeps_the_lock_across_ticks_is_charged_with_each() {
    let (platform, _) = hda_platform();
    map_nvme(&mut platform.lock().unwrap());
    run_stream(&platform, true);
    let holds = PlatformHolds::new();
    let (holds_ref, platform_ref) = (&holds, &platform);
    let mut port = Charged {
        platform: Arc::clone(&platform),
        holds: &holds,
        mem: guest_ram(),
    };
    let (gate, mut counts, start) = (Mutex::new(()), TickCounts::default(), Instant::now());
    tick_once(&mut port, &gate, start, &mut counts).unwrap();
    let (held_tx, held_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    thread::scope(|scope| {
        let vcpu = scope.spawn(move || {
            // A doorbell write whose I/O, a flush say, outlasts 20 ms.
            let doorbell = HoldSite::Mmio(NVME_BAR + 0x1000);
            let held = holds_ref.lock(platform_ref, None, 1, doorbell);
            held_tx.send(()).unwrap();
            thread::sleep(Duration::from_millis(25));
            release_rx.recv().unwrap();
            drop(held);
        });
        held_rx.recv().unwrap();
        for tick in 1..=3 {
            let now = start + TICK_INTERVAL * tick;
            tick_once(&mut port, &gate, now, &mut counts).unwrap();
        }
        release_tx.send(()).unwrap();
        vcpu.join().unwrap();
    });
    tick_once(&mut port, &gate, start + TICK_INTERVAL * 4, &mut counts).unwrap();
    assert_eq!((counts.ticks, counts.busy, counts.max_busy_run), (5, 3, 3));
    let fields = fields(&holds.record());
    assert_eq!((fields["busy"], fields["unattributed"]), (counts.busy, 0));
    let nvme = ["holds", "busy", "gt_20ms"].map(|field| fields[&format!("nvme_{field}")]);
    assert_eq!(nvme, [1, 3, 1], "{fields:?}");
    assert!(fields["nvme_max_us"] >= 25_000, "{fields:?}");
    let others = Holder::ALL.into_iter().filter(|holder| *holder != Holder::Nvme);
    let other_holds: u64 = others.map(|holder| fields[&format!("{}_holds", holder.name())]).sum();
    assert_eq!(other_holds, 0);
}

#[test]
fn the_thread_prints_the_holds_record_right_after_its_stopped_line() {
    let tick = include_str!("hda_service_tick.rs");
    let stopped = "println!(\"{}\", c.stopped_record(interval));";
    let record = "println!(\"{}\", PLATFORM_HOLDS.record());";
    assert_eq!(tick.matches(record).count(), 1);
    let after = &tick[tick.find(stopped).unwrap() + stopped.len()..];
    assert!(after.trim_start().starts_with(record));
}
