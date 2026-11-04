//! Codex P2 on #518: after a 10-minute pause, a new boot in the same helper
//! kept a host-time agent origin and boot-timer origin against VM-time ticks.

use super::agent_console_tests::tests::harness;
use super::state::AgentConsoleState;
use crate::boot_clocks::{agent_origin, tick_times, Clocks};
use crate::BootTimer;
use std::cell::Cell;
use std::time::{Duration, Instant};

/// Host time that the test moves; VM time is behind it by the earlier pauses.
struct Fake {
    host: Cell<Instant>,
    paused: Duration,
}

impl Clocks for Fake {
    fn host(&self) -> Instant {
        self.host.get()
    }

    fn vm(&self) -> Instant {
        self.host.get() - self.paused
    }
}

#[test]
fn a_boot_after_a_ten_minute_pause_keeps_its_ready_timeout_and_desktop_time() {
    let boot = Instant::now() + Duration::from_secs(3_600);
    let clocks = Fake { host: Cell::new(boot), paused: Duration::from_secs(600) };
    let timeout = Duration::from_secs(180);

    // The new boot: the agent console and the boot timer start now.
    let mut agent = harness();
    agent.start = agent_origin(&clocks);
    agent.timeout = timeout;
    agent.state = AgentConsoleState::WaitingReady;
    let mut timer = BootTimer::new(true, clocks.host(), Duration::from_secs(1), None, Vec::new());
    timer.desktop_agent = true;

    // The agent reports READY 42 s into the boot.
    clocks.host.set(boot + Duration::from_secs(42));
    let ready = timer.observe_agent_ready(tick_times(&clocks).telemetry, 7);
    assert_eq!(ready, Some(Duration::from_secs(42)), "the desktop time is this boot's elapsed time");

    // Without READY, the timeout fires at the normal time after the boot's start.
    clocks.host.set(boot + timeout - Duration::from_secs(1));
    agent.advance(tick_times(&clocks).agent, &mut |_| {});
    assert!(matches!(agent.state, AgentConsoleState::WaitingReady), "not before the timeout");
    clocks.host.set(boot + timeout);
    agent.advance(tick_times(&clocks).agent, &mut |_| {});
    assert!(matches!(agent.state, AgentConsoleState::TimedOut), "at the normal timeout, not 10 min later");
}
