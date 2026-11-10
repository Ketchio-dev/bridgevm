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
    assert_eq!(command, time_resync_command(ticks, None));
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
        time_resync_command(638_978_400_000_000_000, None),
        "powershell -NoProfile -NonInteractive -Command \"$ErrorActionPreference='Stop';\
         $t=[DateTime]::new(638978400000000000,'Utc')+((Get-Date)-[Diagnostics.Process]::GetCurrentProcess().StartTime);\
         $null=Set-Date ($t.ToLocalTime())\""
    );
    // cmd.exe's 8,191 characters less bvagent's `"C:\Windows\system32\cmd.exe" /c ""`.
    let limit = 8_191 - r#""C:\Windows\system32\cmd.exe" /c """#.len();
    assert_eq!(limit, 8_156);
    assert!(time_resync_command(u64::MAX, Some("Pacific SA Standard Time (Mexico)")).len() < limit);
}

/// A VM pause stops the guest counter, so Windows' clock is behind by the
/// pause; the host clocks show no gap, so resume queues the resync itself.
#[test]
fn a_vm_pause_queues_one_resync_for_its_length() {
    let (wall, mono) = clocks();
    let mut h = resident((wall, mono));
    h.note_vm_pause(Duration::ZERO);
    assert_eq!(resyncs(&h), 0, "an abandoned pause stopped nothing");
    h.note_vm_pause(Duration::from_secs(60));
    h.note_vm_pause(Duration::from_secs(5));
    assert_eq!(resyncs(&h), 1);
    assert_eq!(h.clock.pending_gap_ms, 65_000);
    let sent = send(&mut h, mono + TICK);
    assert!(sent.iter().any(|line| line.contains("Set-Date") || line.starts_with("RUN ")), "{sent:?}");
    assert_eq!(h.clock.pending_gap_ms, 0, "the sent resync covers both pauses");
}

/// Review on 3d9f317f: live lanes boot test images at a fixed date
/// (BRIDGEVM_GUEST_RTC_EPOCH); a resync keeps the guest on that timeline.
#[test]
fn a_resync_with_a_fixed_guest_epoch_targets_that_date_plus_the_time_since() {
    let mut clock = HostClock::default();
    let host_start = UNIX_EPOCH + Duration::from_secs(1_800_000_000);
    let guest_start = UNIX_EPOCH + Duration::from_secs(1_785_542_400);
    clock.rtc_epoch = Some((guest_start, host_start));
    assert_eq!(
        clock.resync_target(host_start + Duration::from_secs(600)),
        guest_start + Duration::from_secs(600)
    );
    clock.rtc_epoch = None;
    assert_eq!(clock.resync_target(host_start), host_start);
}

#[test]
fn the_first_service_start_of_a_boot_queues_one_resync_and_a_restart_none() {
    // Windows reads the PL031 (UTC) as local time, so in any zone but UTC its clock is off by the
    // zone offset after every boot (live: Korea -9 h, Eastern +4 h), and only a host sleep or a
    // VM pause used to put it right.
    let mut h = harness();
    h.service = true;
    let now = Instant::now();
    assert!(h.next_command_line(h.commands.len(), now).is_none());
    assert!(matches!(h.state, AgentConsoleState::Service));
    assert_eq!(resyncs(&h), 1, "no resync at the boot's service start");
    assert_eq!(h.clock.pending_gap_ms, 0);

    let sent = send(&mut h, now);
    let command = String::from_utf8(base64_decode(sent[0].strip_prefix("RUN ").unwrap().trim_end()).unwrap()).unwrap();
    assert!(command.contains("Set-Date"), "{command}");
    h.handle_service_reply_with_sender("OUT 0 ", now, &mut |_| {});
    assert_eq!(resyncs(&h), 0);

    // An agent restart (an update) in the same boot starts the service again: the clock is set.
    assert!(h.next_command_line(h.commands.len(), now).is_none());
    assert_eq!(resyncs(&h), 0);
}

/// The resync sets the Mac's time zone first: the zone step can fail without
/// stopping Set-Date, and .NET's cached zone is cleared before the conversion.
#[test]
fn the_resync_sets_the_macs_zone_before_the_clock() {
    let command = time_resync_command(638_978_400_000_000_000, Some("Korea Standard Time"));
    let zone = command.find("Set-TimeZone -Id $z").unwrap();
    let clear = command.find("[TimeZoneInfo]::ClearCachedData()").unwrap();
    let catch = command.find("catch{'BVTZ failed '").unwrap();
    let local = command.find("$t.ToLocalTime()").unwrap();
    assert!(command.contains("$z='Korea Standard Time'"));
    assert!(zone < clear && clear < catch && catch < local, "{command}");
}

/// The boot's resync carries the Mac's zone; a later Mac zone change queues
/// one more resync with the new zone, and an unchanged zone queues none.
#[test]
fn a_mac_zone_change_queues_a_resync_with_the_new_zone() {
    let sent_command = |sent: Vec<String>| {
        let line = sent.iter().find(|line| line.starts_with("RUN ")).expect("a RUN line");
        String::from_utf8(base64_decode(line["RUN ".len()..].trim_end()).unwrap()).unwrap()
    };
    let (wall, mono) = clocks();
    let mut h = resident((wall, mono));
    h.clock.zone.host = Some(Some("Korea Standard Time"));
    h.enqueue_boot_time_resync();
    assert!(sent_command(send(&mut h, mono)).contains("$z='Korea Standard Time'"));
    h.complete_in_flight();

    // The same zone a minute later: nothing due.
    h.clock.zone.tick(mono + ZONE_TICK);
    h.enqueue_time_resync();
    assert_eq!(resyncs(&h), 0);

    // The Mac moved to another zone: one resync, carrying it.
    h.clock.zone.host = Some(Some("Eastern Standard Time"));
    h.clock.zone.tick(mono + ZONE_TICK * 2);
    h.enqueue_time_resync();
    h.enqueue_time_resync();
    assert_eq!(resyncs(&h), 1);
    assert!(sent_command(send(&mut h, mono + ZONE_TICK * 2)).contains("$z='Eastern Standard Time'"));
    assert!(!h.clock.zone.changed);
}

/// An unknown Mac zone leaves Windows' zone alone: today's command exactly,
/// and no zone change is ever seen.
#[test]
fn an_unknown_mac_zone_sends_the_plain_resync() {
    let (wall, mono) = clocks();
    let mut h = resident((wall, mono));
    h.enqueue_boot_time_resync();
    let sent = send(&mut h, mono);
    assert!(sent.iter().any(|line| line.starts_with("RUN ")), "{sent:?}");
    assert_eq!(h.clock.zone.applied, None);
    h.clock.zone.host = Some(Some("Korea Standard Time"));
    h.clock.zone.tick(mono + ZONE_TICK);
    assert!(!h.clock.zone.changed, "nothing was applied, so nothing changed");
}

const ZONE_TICK: Duration = super::super::time_zone::ZONE_CHECK;
