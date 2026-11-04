//! Desktop switch requests: one pending switch with the latest size, and the
//! script that applies it in the guest.

use super::super::agent_console_tests::tests::harness;
use super::super::*;
use super::*;

pub(crate) fn decoded(command: &str) -> String {
    let b64 = command.rsplit(' ').next().unwrap();
    let bytes = base64_decode(b64).unwrap();
    let units: Vec<u16> = bytes
        .chunks(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    String::from_utf16(&units).unwrap()
}

/// The decoded scripts of the queued desktop switches.
fn queued_switches(h: &AgentConsoleHarness) -> Vec<String> {
    h.queue
        .iter()
        .filter_map(|req| match req {
            ServiceReq::Ctl(c) if is_desktop_switch_command(c) => Some(decoded(c)),
            _ => None,
        })
        .collect()
}

pub(crate) fn service_harness() -> AgentConsoleHarness {
    let mut h = harness();
    h.commands.clear();
    h.service = true;
    h
}

#[test]
fn requests_before_the_service_start_become_one_switch_with_the_last_size() {
    let mut h = service_harness();
    request_desktop_mode((1600, 900));
    request_desktop_mode((1920, 1080));
    let now = Instant::now();
    assert_eq!(h.next_command_line(0, now), None);
    assert!(matches!(h.state, AgentConsoleState::Service));
    h.service_enqueue(now);
    let switches = queued_switches(&h);
    assert_eq!(switches.len(), 1, "one switch at service start");
    assert!(switches[0].ends_with("[BvD]::Run(1920,1080)"));

    // A request before that one is sent replaces it.
    request_desktop_mode((1517, 931));
    h.service_enqueue(now);
    let switches = queued_switches(&h);
    assert_eq!(switches.len(), 1);
    assert!(switches[0].ends_with("[BvD]::Run(1517,931)"));
}

#[test]
fn requests_while_a_switch_runs_give_exactly_one_follow_up() {
    let mut h = service_harness();
    h.state = AgentConsoleState::Service;
    let now = Instant::now();
    request_desktop_mode((1600, 900));
    h.service_enqueue(now);
    let running = h.queue.pop_front().expect("switch queued");
    h.in_flight = Some((running, now));

    request_desktop_mode((1920, 1080));
    request_desktop_mode((1517, 931));
    h.service_enqueue(now);
    assert!(queued_switches(&h).is_empty(), "none queued while one runs");

    h.complete_in_flight();
    h.service_enqueue(now);
    h.service_enqueue(now);
    let switches = queued_switches(&h);
    assert_eq!(switches.len(), 1, "exactly one follow-up");
    assert!(switches[0].ends_with("[BvD]::Run(1517,931)"));
    assert!(!has_requested_mode());
}

#[test]
fn the_script_targets_the_virtio_display_fixes_the_layout_and_retries() {
    request_desktop_mode((1517, 931));
    let command = take_desktop_switch_command().expect("requested");
    assert_eq!(take_desktop_switch_command(), None, "only once");
    assert!(command.starts_with("powershell -NoProfile -NonInteractive"));
    assert!(command.len() <= 8_156, "{} chars", command.len());
    assert!(is_desktop_switch_command(&command));
    assert!(!is_desktop_switch_command(
        "powershell -NoProfile -NonInteractive -Command whoami"
    ));
    let script = decoded(&command);
    for needle in [
        // viogpudo's display, by adapter ID; otherwise the primary as before.
        r#"T(b,328,256).Contains("VEN_1AF4&DEV_1050")"#,
        r#"if(t==null){res="0";if(S(t)!=want){var m=M(null,-1)"#,
        // Layout: virtio primary at 0,0, other attached displays 0x0, apply.
        "P(cur,76,0);P(cur,80,0);lay=\"\"+C(V,cur,0x10000011)",
        "P(z,72,0x180020);det.Add(o+\"=\"+C(o,z,0x10000001))",
        "C(null,null,0)",
        // Until WxH is listed: re-enumerate the active configuration's modes.
        "QueryDisplayConfig(2,ref p,a,ref m,b,IntPtr.Zero)",
        "SetDisplayConfig(p,a,m,b,0x80|0x20|0x400|0x1000)",
        "AddSeconds(10)",
        "Thread.Sleep(400)",
        // Select the listed mode and save it.
        "if(G(m,172)==w&&G(m,176)==h)f=m",
        "C(t,f,1)",
        "\"BVDISPLAY target=\"+want",
        "[BvD]::Run(1517,931)",
    ] {
        assert!(script.contains(needle), "missing {needle}");
    }
}

#[test]
fn the_longest_switch_fits_cmd_exe_with_the_agent_wrapper() {
    // cmd.exe takes 8,191 characters, 35 of them the agent's
    // `"C:\Windows\system32\cmd.exe" /c "..."` around the command.
    assert_eq!(MAX_RUN_COMMAND_CHARS, 8_156);
    let command = desktop_switch_command(u32::MAX, u32::MAX);
    assert!(command.len() <= 8_156, "{} chars", command.len());
}
