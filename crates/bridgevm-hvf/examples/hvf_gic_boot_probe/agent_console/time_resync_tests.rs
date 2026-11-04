//! Host suspension detection and the guest clock resync it queues.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

const TICK: Duration = Duration::from_millis(250);

fn clocks() -> (SystemTime, Instant) {
    (UNIX_EPOCH + Duration::from_secs(1_800_000_000), Instant::now())
}

/// A resident harness whose detector has seen one tick at the injected pair.
fn resident((wall, mono): (SystemTime, Instant)) -> AgentConsoleHarness {
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.service = true;
    assert!(!h.clock.observe(wall, mono));
    h
}

fn resyncs(h: &AgentConsoleHarness) -> usize {
    let in_flight = h.in_flight.iter().map(|(req, _)| req);
    h.queue.iter().chain(in_flight).filter(|req| is_time_resync(req)).count()
}

fn send(h: &mut AgentConsoleHarness, now: Instant) -> Vec<String> {
    let mut sent = Vec::new();
    h.service_pump_with_sender(now, &mut |bytes| {
        sent.push(String::from_utf8(bytes.to_vec()).unwrap())
    });
    sent
}

#[test]
fn ticks_without_a_gap_queue_no_resync() {
    let (wall, mono) = clocks();
    let mut h = resident((wall, mono));
    for n in 1..=40 {
        // A 1.5 s host clock step at tick 20 stays under the threshold.
        let step = if n >= 20 { Duration::from_millis(1_500) } else { Duration::ZERO };
        assert!(!h.clock.observe(wall + TICK * n + step, mono + TICK * n));
        h.enqueue_time_resync();
    }
    assert_eq!(h.clock.pending_gap_ms, 0);
    assert_eq!(resyncs(&h), 0);
}

#[test]
fn a_ten_second_gap_sends_exactly_one_resync_at_host_time() {
    let (wall, mono) = clocks();
    let mut h = resident((wall, mono));
    assert!(h.clock.observe(wall + Duration::from_secs(10) + TICK, mono + TICK));
    assert_eq!(h.clock.pending_gap_ms, 10_000);
    h.enqueue_time_resync();
    h.enqueue_time_resync();
    assert_eq!(resyncs(&h), 1);

    let sent = send(&mut h, mono + TICK);
    assert_eq!(sent.len(), 1);
    let b64 = sent[0].strip_prefix("RUN ").unwrap().trim_end();
    let command = String::from_utf8(base64_decode(b64).unwrap()).unwrap();
    let ticks: u64 = command
        .split("::new(")
        .nth(1)
        .and_then(|rest| rest.split(',').next())
        .unwrap()
        .parse()
        .unwrap();
    assert_eq!(command, time_resync_command(ticks));
    // The real host clock, not the injected pair.
    let host = SystemTime::now().duration_since(UNIX_EPOCH).unwrap();
    let host_ticks = UNIX_EPOCH_TICKS + (host.as_nanos() / 100) as u64;
    assert!(ticks.abs_diff(host_ticks) < 50_000_000, "{ticks} vs {host_ticks}");
    assert_eq!(h.clock.pending_gap_ms, 0);

    h.enqueue_time_resync();
    assert_eq!(resyncs(&h), 1);
    h.handle_service_reply_with_sender("OUT 0 ", mono + TICK, &mut |_| {});
    assert_eq!(resyncs(&h), 0);
    assert!(h.in_flight.is_none());
}

#[test]
fn repeated_gaps_coalesce_and_one_in_flight_gets_one_follow_up() {
    let (wall, mono) = clocks();
    let mut h = resident((wall, mono));
    let mut sleep = Duration::ZERO;
    let mut gap = |h: &mut AgentConsoleHarness, n: u32| {
        sleep += Duration::from_secs(10);
        assert!(h.clock.observe(wall + sleep + TICK * n, mono + TICK * n));
        h.enqueue_time_resync();
    };
    gap(&mut h, 1);
    gap(&mut h, 2);
    assert_eq!(resyncs(&h), 1);
    assert_eq!(h.clock.pending_gap_ms, 20_000);

    assert_eq!(send(&mut h, mono).len(), 1);
    assert!(matches!(h.in_flight, Some((ServiceReq::TimeSync, _))));
    gap(&mut h, 3);
    gap(&mut h, 4);
    assert_eq!(h.queue.len(), 1);
    assert_eq!(resyncs(&h), 2);
}

#[test]
fn the_test_env_injects_one_gap_when_due() {
    assert_eq!(
        parse_test_suspend("10000@60000"),
        Some((Duration::from_secs(10), Duration::from_secs(60)))
    );
    for rejected in ["", "10000", "x@1", "1@", "-1@5"] {
        assert_eq!(parse_test_suspend(rejected), None, "{rejected}");
    }
    let (wall, mono) = clocks();
    let mut clock = HostClock {
        test_suspend: Some((Duration::from_secs(10), Duration::from_secs(1))),
        ..HostClock::default()
    };
    let detected: Vec<bool> = (0..8u32)
        .map(|n| clock.tick(wall + TICK * n, mono + TICK * n, TICK * n))
        .collect();
    assert_eq!(detected.iter().filter(|seen| **seen).count(), 1);
    assert!(detected[4]);
    assert_eq!(clock.pending_gap_ms, 10_000);
}

#[test]
fn resync_command_is_fixed_text_plus_the_host_ticks() {
    assert_eq!(
        time_resync_command(638_978_400_000_000_000),
        "powershell -NoProfile -NonInteractive -Command \"$ErrorActionPreference='Stop';\
         $t=[DateTime]::new(638978400000000000,'Utc')+((Get-Date)-[Diagnostics.Process]::GetCurrentProcess().StartTime);\
         $null=Set-Date ($t.ToLocalTime())\""
    );
    // cmd.exe's 8,191 characters less bvagent's `"C:\Windows\system32\cmd.exe" /c ""`.
    let limit = 8_191 - r#""C:\Windows\system32\cmd.exe" /c """#.len();
    assert_eq!(limit, 8_156);
    assert!(time_resync_command(u64::MAX).len() < limit);
}
