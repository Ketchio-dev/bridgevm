//! Puts the guest's clock back on host time after the Mac sleeps.
//!
//! The guest counter is `mach_absolute_time()` minus the vTimer offset
//! (hv_vcpu.h), and that clock stops while the Mac sleeps, so Windows' wall
//! clock falls behind by every host sleep. Windows reads the RTC only at boot
//! and W32Time syncs about weekly on a workgroup machine. `SystemTime` keeps
//! counting through a sleep and `Instant` (CLOCK_UPTIME_RAW) does not, so a
//! tick whose wall-clock step exceeds its monotonic step by more than
//! `RESYNC_GAP` saw a suspension. Once the agent service runs, one `Set-Date`
//! puts the guest on the host's UTC time, read from the host clock when the
//! command is sent. The command is fixed text plus that one integer.

use super::*;
use std::time::{SystemTime, UNIX_EPOCH};

/// Longer than any scheduling stall or clock slew between 250 ms service ticks.
const RESYNC_GAP: Duration = Duration::from_secs(2);

/// From the host's send to the guest PowerShell's start (wire, agent, cmd.exe).
/// The command adds PowerShell's own start-up time itself.
const SEND_LATENCY: Duration = Duration::from_millis(100);

/// .NET ticks (100 ns since 0001-01-01) at the Unix epoch.
const UNIX_EPOCH_TICKS: u64 = 621_355_968_000_000_000;

/// Test only: `GAP_MS@T_MS` makes the detector see one GAP_MS suspension at
/// T_MS on the harness `t=` timeline.
const TEST_SUSPEND_ENV: &str = "BRIDGEVM_TEST_SUSPEND_GAP_MS";

#[derive(Default)]
pub(super) struct HostClock {
    last: Option<(SystemTime, Instant)>,
    /// Suspension (ms) no sent resync has covered yet; 0 when none is due.
    pub(super) pending_gap_ms: u64,
    /// The synthetic gap and when it appears.
    pub(super) test_suspend: Option<(Duration, Duration)>,
    /// BRIDGEVM_GUEST_RTC_EPOCH: the guest's fixed start time, and the host
    /// time it was taken at.
    pub(super) rtc_epoch: Option<(SystemTime, SystemTime)>,
}

impl HostClock {
    pub(super) fn from_env() -> Self {
        Self {
            test_suspend: std::env::var(TEST_SUSPEND_ENV)
                .ok()
                .and_then(|value| parse_test_suspend(&value)),
            rtc_epoch: std::env::var("BRIDGEVM_GUEST_RTC_EPOCH")
                .ok()
                .and_then(|value| value.parse::<u32>().ok().filter(|&seconds| seconds > 0))
                .map(|seconds| {
                    let start = UNIX_EPOCH + Duration::from_secs(seconds.into());
                    (start, SystemTime::now())
                }),
            ..Self::default()
        }
    }

    /// The time a resync sets: host time, or with BRIDGEVM_GUEST_RTC_EPOCH (a
    /// test image booted at a fixed date) that date plus the wall time since,
    /// so the guest stays on its own timeline.
    pub(super) fn resync_target(&self, now: SystemTime) -> SystemTime {
        match self.rtc_epoch {
            Some((guest_start, host_start)) => {
                guest_start + now.duration_since(host_start).unwrap_or_default()
            }
            None => now,
        }
    }

    /// One tick: the clock pair, plus the test gap once it is due.
    pub(super) fn tick(&mut self, mut wall: SystemTime, mono: Instant, elapsed: Duration) -> bool {
        if let Some((gap, at)) = self.test_suspend {
            if elapsed >= at && self.last.is_some() {
                self.test_suspend = None;
                wall += gap;
            }
        }
        self.observe(wall, mono)
    }

    /// Fold in one tick's clock pair; true when it shows a host suspension.
    pub(super) fn observe(&mut self, wall: SystemTime, mono: Instant) -> bool {
        let Some((last_wall, last_mono)) = self.last.replace((wall, mono)) else {
            return false;
        };
        let gap = wall
            .duration_since(last_wall)
            .ok()
            .and_then(|step| step.checked_sub(mono.saturating_duration_since(last_mono)));
        match gap {
            Some(gap) if gap > RESYNC_GAP => {
                self.pending_gap_ms = self.pending_gap_ms.saturating_add(gap.as_millis() as u64);
                true
            }
            _ => false,
        }
    }
}

pub(super) fn parse_test_suspend(value: &str) -> Option<(Duration, Duration)> {
    let (gap, at) = value.trim().split_once('@')?;
    Some((
        Duration::from_millis(gap.parse().ok()?),
        Duration::from_millis(at.parse().ok()?),
    ))
}

/// Windows' own time setting. `Set-Date` takes local time, so the UTC ticks
/// are converted in the guest, after adding the guest PowerShell's start-up.
pub(super) fn time_resync_command(utc_ticks: u64) -> String {
    format!(
        "powershell -NoProfile -NonInteractive -Command \"$ErrorActionPreference='Stop';\
         $t=[DateTime]::new({utc_ticks},'Utc')+((Get-Date)-[Diagnostics.Process]::GetCurrentProcess().StartTime);\
         $null=Set-Date ($t.ToLocalTime())\""
    )
}

fn is_time_resync(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::TimeSync)
}

impl AgentConsoleHarness {
    /// Every tick in every state, so a sleep during boot is caught too. Both
    /// clocks are read here, together: the caller's `now` may predate a wait.
    pub(super) fn observe_host_clock(&mut self) {
        let (wall, mono) = (SystemTime::now(), Instant::now());
        let elapsed = mono.saturating_duration_since(self.start);
        self.clock.tick(wall, mono, elapsed);
    }

    /// At most one queued resync. Its time is read when it is sent, so a
    /// queued one already covers a newer gap; one in flight gets a follow-up.
    pub(super) fn enqueue_time_resync(&mut self) {
        if self.clock.pending_gap_ms > 0 && !self.queue.iter().any(is_time_resync) {
            self.queue.push_back(ServiceReq::TimeSync);
        }
    }

    /// A VM pause stopped the guest counter, so Windows' wall clock is behind
    /// by `paused`, as after a host sleep that the tick cannot see.
    pub(crate) fn note_vm_pause(&mut self, paused: std::time::Duration) {
        if paused.is_zero() {
            return;
        }
        let paused_ms = u64::try_from(paused.as_millis()).unwrap_or(u64::MAX);
        self.clock.pending_gap_ms = self.clock.pending_gap_ms.saturating_add(paused_ms);
        self.enqueue_time_resync();
    }

    /// An agent restart may have dropped the resync before it ran.
    pub(super) fn requeue_time_resync(&mut self) {
        if !self.queue.iter().any(is_time_resync) {
            self.queue.push_front(ServiceReq::TimeSync);
        }
    }

    pub(super) fn write_time_resync_line(&mut self, now: Instant) {
        let set = self
            .clock
            .resync_target(SystemTime::now())
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            + SEND_LATENCY;
        println!(
            "BVTIME resync gap_ms={} set_unix_ms={} t={}",
            self.clock.pending_gap_ms,
            set.as_millis(),
            self.t_ms(now)
        );
        self.clock.pending_gap_ms = 0;
        let ticks = UNIX_EPOCH_TICKS + (set.as_nanos() / 100) as u64;
        write_command_wire_line_into(&time_resync_command(ticks), &mut self.service_line_scratch);
    }

    pub(super) fn handle_time_resync_reply(&mut self, line: &str) {
        if let Some((exit_code, output)) = parse_out_line(line) {
            if exit_code != 0 {
                let text = base64_decode(output).unwrap_or_default();
                let text = String::from_utf8_lossy(&text);
                let text: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
                println!(
                    "BVTIME resync failed exit={exit_code} {}",
                    text.chars().take(200).collect::<String>()
                );
            }
            self.complete_in_flight();
        } else if line.starts_with("OUTEND ") {
            self.complete_in_flight();
        }
    }
}

#[cfg(test)]
#[path = "time_resync_tests.rs"]
mod tests;
