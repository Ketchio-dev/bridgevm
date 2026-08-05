//! The sampling loop that decides a boot has stalled.

use super::*;

/// Sample `watchdog` until it is disarmed, reporting the first sustained stall.
pub(crate) fn spawn_boot_progress_watchdog(
    watchdog: Arc<BootProgressWatchdog>,
    min_exits_per_sec: u64,
    stall_after: Duration,
    label: &'static str,
    kill: Option<BootProgressKill>,
) {
    std::thread::spawn(move || {
        let mut under_floor = Duration::ZERO;
        let mut window_start_exits = watchdog.exits();
        while watchdog.is_armed() {
            std::thread::sleep(PROGRESS_SAMPLE_INTERVAL);
            if !watchdog.is_armed() {
                return;
            }
            let now_exits = watchdog.exits();
            let delta = now_exits.saturating_sub(window_start_exits);
            if is_stalled(delta, PROGRESS_SAMPLE_INTERVAL, min_exits_per_sec) {
                under_floor += PROGRESS_SAMPLE_INTERVAL;
            } else {
                under_floor = Duration::ZERO;
            }
            window_start_exits = now_exits;

            if under_floor >= stall_after && watchdog.mark_fired() {
                let record = BootProgressRecord {
                    stalled_for: under_floor,
                    exits_in_window: delta,
                    total_exits: now_exits,
                    reboots: watchdog.reboots(),
                };
                println!("{}", record.format(label));
                if let Some(kill) = kill {
                    // Snapshot at the moment the stall is confirmed, not after
                    // the run unwinds: t4-soak says every A1 failure lands
                    // here, at the first reboot.
                    for line in crate::stall_gic_report(kill.vcpu, record.reboots) {
                        println!("{label}: {line}");
                    }
                    println!("{label}: boot-progress watchdog ending run (kill mode)");
                    kill.fire();
                    return;
                }
            }
        }
    });
}
