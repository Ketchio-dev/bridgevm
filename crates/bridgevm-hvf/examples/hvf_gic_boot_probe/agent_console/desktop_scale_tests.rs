//! Display scale requests: one pending scale with the latest value, set after
//! every desktop switch, and the script that sets it in the guest.

use super::super::desktop_switch::tests::{decoded, service_harness};
use super::super::desktop_switch::{
    desktop_switch_run_command, is_desktop_switch_command, request_desktop_mode,
    take_desktop_switch_command, MAX_RUN_COMMAND_CHARS,
};
use super::super::*;
use super::*;

fn is_switch_or_scale(req: &ServiceReq) -> bool {
    matches!(req, ServiceReq::Ctl(c) if is_desktop_switch_command(c) || is_desktop_scale_command(c))
}

fn queued_switches(h: &AgentConsoleHarness) -> usize {
    h.queue
        .iter()
        .filter(|req| matches!(req, ServiceReq::Ctl(c) if is_desktop_switch_command(c)))
        .count()
}

/// The queued scale commands.
fn queued_scales(h: &AgentConsoleHarness) -> Vec<String> {
    h.queue
        .iter()
        .filter_map(|req| match req {
            ServiceReq::Ctl(c) if is_desktop_scale_command(c) => Some(c.clone()),
            _ => None,
        })
        .collect()
}

/// Send the first queued switch or scale, as the service pump would.
fn send_next(h: &mut AgentConsoleHarness, now: Instant) {
    let index = h.queue.iter().position(is_switch_or_scale).expect("queued");
    h.in_flight = Some((h.queue.remove(index).unwrap(), now));
}

/// Run every queued switch and scale to completion.
fn run_all(h: &mut AgentConsoleHarness, now: Instant) {
    h.service_enqueue(now);
    while h.queue.iter().any(is_switch_or_scale) {
        send_next(h, now);
        h.complete_in_flight();
        h.service_enqueue(now);
    }
}

fn running_harness() -> AgentConsoleHarness {
    let mut h = service_harness();
    h.state = AgentConsoleState::Service;
    h
}

#[test]
fn a_scale_waits_until_its_switch_is_done() {
    let mut h = running_harness();
    let now = Instant::now();
    request_desktop_mode((2560, 1600));
    request_desktop_scale(200);
    h.service_enqueue(now);
    assert_eq!(queued_switches(&h), 1);
    assert!(queued_scales(&h).is_empty(), "not before the switch");
    send_next(&mut h, now);
    h.service_enqueue(now);
    assert!(queued_scales(&h).is_empty(), "nor while it runs");

    h.complete_in_flight();
    h.service_enqueue(now);
    h.service_enqueue(now);
    let scales = queued_scales(&h);
    assert_eq!(scales.len(), 1, "once");
    assert_eq!(scales[0], "DISPLAYSCALE 200");
}

#[test]
fn every_switch_sets_the_latest_scale_again() {
    let mut h = running_harness();
    let now = Instant::now();
    request_desktop_mode((2560, 1600));
    request_desktop_scale(200);
    run_all(&mut h, now);
    assert!(!has_requested_scale());

    // A size without a scale: the new monitor gets 200 again after the switch.
    request_desktop_mode((1920, 1080));
    h.service_enqueue(now);
    assert!(queued_scales(&h).is_empty());
    send_next(&mut h, now);
    h.complete_in_flight();
    h.service_enqueue(now);
    let scales = queued_scales(&h);
    assert_eq!(scales.len(), 1);
    assert_eq!(scales[0], "DISPLAYSCALE 200");
}

#[test]
fn a_switch_requested_meanwhile_goes_first_and_the_newest_scale_wins() {
    let mut h = running_harness();
    let now = Instant::now();
    request_desktop_mode((2560, 1600));
    request_desktop_scale(200);
    h.service_enqueue(now);
    send_next(&mut h, now);

    // Requests while the switch runs: its follow-up runs before the scale.
    request_desktop_mode((3456, 2234));
    request_desktop_scale(150);
    request_desktop_scale(175);
    h.service_enqueue(now);
    h.complete_in_flight();
    h.service_enqueue(now);
    assert_eq!(queued_switches(&h), 1, "the follow-up switch");
    assert!(queued_scales(&h).is_empty());
    send_next(&mut h, now);
    h.complete_in_flight();
    h.service_enqueue(now);
    let scales = queued_scales(&h);
    assert_eq!(scales.len(), 1);
    assert_eq!(scales[0], "DISPLAYSCALE 175");

    // A scale still queued when a new switch comes would set the old monitor;
    // it moves behind the switch.
    request_desktop_mode((1280, 800));
    h.service_enqueue(now);
    assert_eq!(queued_switches(&h), 1);
    assert!(queued_scales(&h).is_empty());
    send_next(&mut h, now);
    h.complete_in_flight();
    h.service_enqueue(now);
    assert_eq!(queued_scales(&h).len(), 1);
}

#[test]
fn requests_while_a_scale_runs_give_exactly_one_follow_up() {
    let mut h = running_harness();
    let now = Instant::now();
    request_desktop_scale(200);
    h.service_enqueue(now);
    send_next(&mut h, now);

    request_desktop_scale(150);
    request_desktop_scale(175);
    h.service_enqueue(now);
    assert!(queued_scales(&h).is_empty(), "none queued while one runs");

    h.complete_in_flight();
    h.service_enqueue(now);
    h.service_enqueue(now);
    let scales = queued_scales(&h);
    assert_eq!(scales.len(), 1, "exactly one follow-up");
    assert_eq!(scales[0], "DISPLAYSCALE 175");
    assert!(!has_requested_scale());
}

#[test]
fn the_script_sets_the_virtio_display_scale_only_when_it_differs() {
    for percent in DESKTOP_SCALES {
        assert_eq!(
            desktop_scale_command(percent),
            format!("DISPLAYSCALE {percent}")
        );
        let command = desktop_scale_run_command(percent);
        assert!(command.len() <= 8_156, "{} chars", command.len());
        assert!(command.len() <= MAX_RUN_COMMAND_CHARS);
        for command in [command, desktop_scale_command(percent)] {
            assert!(is_desktop_scale_command(&command));
            assert!(!is_desktop_switch_command(&command));
        }
    }
    request_desktop_mode((1517, 931));
    let switch = take_desktop_switch_command().expect("requested");
    assert!(!is_desktop_scale_command(&switch));
    assert!(!is_desktop_scale_command(&desktop_switch_run_command(
        1517, 931
    )));

    let script = decoded(&desktop_scale_run_command(200));
    let ladder = DESKTOP_SCALES.map(|p| p.to_string()).join(",");
    for needle in [
        format!("static int[] L={{{ladder}}};"),
        // The active path whose source adapter is the virtio-gpu.
        "QueryDisplayConfig(2,ref p,a,ref m,b,IntPtr.Zero)".into(),
        "Array.Copy(a,i*72,S,0,12);var r=Q(4,276)".into(),
        r#"GetString(r,20,256).ToUpperInvariant().Contains("VEN_1AF4&DEV_1050")"#.into(),
        // Read (-3), clamp to the maximum, set (-4) only if it differs, read back.
        "var r=Q(-3,32)".into(),
        "Math.Min(Array.IndexOf(L,w),z+d[2])".into(),
        "if(t-z!=k){var r=Q(-4,24);BitConverter.GetBytes(t-z).CopyTo(r,20)".into(),
        "var e=D();".into(),
        r#"return "BVDPI target="+w+" before="+X(z+k)+" after="+"#.into(),
        r#"+" rec="+X(z)+" max="+X(z+d[2])+" result="+res;"#.into(),
    ] {
        assert!(script.contains(&needle), "missing {needle}");
    }
    assert!(script.ends_with("}}';[BvS]::Run(200)"));
}
