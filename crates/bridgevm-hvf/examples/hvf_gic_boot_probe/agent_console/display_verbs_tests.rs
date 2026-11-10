//! The desktop switch and display scale go to the agent as resident verbs, and
//! as RUN powershell lines to an agent without them.

use super::super::desktop_scale::request_desktop_scale;
use super::super::desktop_switch::request_desktop_mode;
use super::super::desktop_switch::tests::{decoded, service_harness};
use super::super::*;
use super::*;

const SWITCHED: &str = r"BVDISPLAY target=1920x1080 before=1280x800 after=1920x1080 primary=\\.\DISPLAY1 layout=none detached=none force=none result=0";
const SCALED: &str = "BVDPI target=150 before=100 after=150 rec=100 max=300 result=0";

/// An agent reply as Write-CommandResult frames it.
fn out(exit: i32, text: &str) -> String {
    let mut line = format!("OUT {exit} ");
    base64_encode_into(text.as_bytes(), &mut line);
    line
}

fn running() -> (AgentConsoleHarness, Instant) {
    let mut h = service_harness();
    h.state = AgentConsoleState::Service;
    let now = Instant::now();
    // No heartbeat PING between the requests.
    h.last_send = Some(now);
    (h, now)
}

/// The next line the service sends, after folding in the requests.
fn next_wire(h: &mut AgentConsoleHarness, now: Instant) -> Option<String> {
    h.service_enqueue(now);
    let mut sent = None;
    h.service_pump_with_sender(now, &mut |bytes| {
        sent = Some(String::from_utf8_lossy(bytes).into_owned());
    });
    sent
}

fn reply(h: &mut AgentConsoleHarness, line: &str, now: Instant) {
    h.handle_service_reply_with_sender(line, now, &mut |_| panic!("a reply sent a line"));
}

/// The PowerShell script a `RUN <b64(powershell ... -EncodedCommand ...)>` line runs.
fn run_script(wire: &str) -> String {
    let run = wire
        .trim_end()
        .strip_prefix("RUN ")
        .unwrap_or_else(|| panic!("not a RUN line: {wire}"));
    decoded(&String::from_utf8(base64_decode(run).unwrap()).unwrap())
}

#[test]
fn a_resize_goes_to_the_agent_as_resident_verbs() {
    let (mut h, now) = running();
    request_desktop_mode((1920, 1080));
    request_desktop_scale(150);
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1920 1080\n")
    );
    assert_eq!(
        next_wire(&mut h, now),
        None,
        "lockstep: nothing else while it runs"
    );
    reply(&mut h, &out(0, &format!("{SWITCHED}\r\n")), now);
    assert!(h.in_flight.is_none(), "the BVDISPLAY reply completes it");
    // The scale is set once the switch is done.
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYSCALE 150\n")
    );
    reply(&mut h, &out(0, &format!("{SCALED}\r\n")), now);
    assert!(h.in_flight.is_none());
    assert_eq!(next_wire(&mut h, now), None);
}

#[test]
fn an_agent_without_the_verbs_gets_run_lines_for_that_request_and_later_ones() {
    let (mut h, now) = running();
    request_desktop_mode((1920, 1080));
    request_desktop_scale(150);
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1920 1080\n")
    );
    // bvagent.ps1's default branch.
    reply(&mut h, &out(255, "unknown token: DISPLAYMODE"), now);
    let wire = next_wire(&mut h, now).expect("the same request again");
    assert!(
        run_script(&wire).ends_with("[BvD]::Run(1920,1080)"),
        "{wire}"
    );
    reply(&mut h, &out(0, &format!("{SWITCHED}\r\n")), now);
    let wire = next_wire(&mut h, now).expect("then the scale");
    assert!(
        run_script(&wire).ends_with("[BvS]::Run(150)"),
        "the scale skips the verb too"
    );
    reply(&mut h, &out(0, &format!("{SCALED}\r\n")), now);

    // A later resize, and the scale after it, go the RUN way at once.
    request_desktop_mode((1280, 800));
    let wire = next_wire(&mut h, now).expect("the next switch");
    assert!(run_script(&wire).ends_with("[BvD]::Run(1280,800)"));
    reply(
        &mut h,
        &out(0, "BVDISPLAY target=1280x800 result=0\r\n"),
        now,
    );
    let wire = next_wire(&mut h, now).expect("its scale");
    assert!(run_script(&wire).ends_with("[BvS]::Run(150)"));
    reply(&mut h, &out(0, &format!("{SCALED}\r\n")), now);
    assert_eq!(next_wire(&mut h, now), None);

    // A restarted agent (a guest tools update) may have them: try the verb again.
    reply(&mut h, "READY BRIDGEVM v3-share2 putdata bigfile", now);
    request_desktop_mode((1600, 900));
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1600 900\n")
    );
}

#[test]
fn a_verb_the_agent_could_not_compile_falls_back_like_an_unknown_one() {
    let (mut h, now) = running();
    request_desktop_scale(200);
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYSCALE 200\n")
    );
    reply(&mut h, "ERR DISPLAYSCALE unavailable", now);
    let wire = next_wire(&mut h, now).expect("the same scale again");
    assert!(run_script(&wire).ends_with("[BvS]::Run(200)"));
    reply(&mut h, &out(0, "BVDPI target=200 result=0\r\n"), now);
    request_desktop_mode((1920, 1080));
    let wire = next_wire(&mut h, now).expect("a switch");
    assert!(run_script(&wire).ends_with("[BvD]::Run(1920,1080)"));
}

#[test]
fn the_verb_path_keeps_the_latest_size_with_one_follow_up() {
    let (mut h, now) = running();
    request_desktop_mode((1600, 900));
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1600 900\n")
    );
    request_desktop_mode((1920, 1080));
    request_desktop_mode((1517, 931));
    assert_eq!(next_wire(&mut h, now), None, "none while one runs");
    reply(
        &mut h,
        &out(0, "BVDISPLAY target=1600x900 result=0\r\n"),
        now,
    );
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1517 931\n"),
        "one follow-up, the latest"
    );
    reply(
        &mut h,
        &out(0, "BVDISPLAY target=1517x931 result=0\r\n"),
        now,
    );
    assert_eq!(next_wire(&mut h, now), None);

    // A refusal while a newer size waits: the newer one goes, as RUN, and only it.
    request_desktop_mode((1280, 800));
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1280 800\n")
    );
    request_desktop_mode((2560, 1600));
    reply(&mut h, &out(255, "unknown token: DISPLAYMODE"), now);
    let wire = next_wire(&mut h, now).expect("the latest size");
    assert!(
        run_script(&wire).ends_with("[BvD]::Run(2560,1600)"),
        "{}",
        run_script(&wire)
    );
    reply(
        &mut h,
        &out(0, "BVDISPLAY target=2560x1600 result=0\r\n"),
        now,
    );
    assert_eq!(next_wire(&mut h, now), None);
}

#[test]
fn a_size_outside_the_verbs_range_goes_as_run_and_keeps_the_verb() {
    let (mut h, now) = running();
    request_desktop_mode((300, 200));
    let wire = next_wire(&mut h, now).expect("a switch");
    assert!(run_script(&wire).ends_with("[BvD]::Run(300,200)"));
    reply(
        &mut h,
        &out(0, "BVDISPLAY target=300x200 result=not_listed\r\n"),
        now,
    );
    request_desktop_mode((1920, 1080));
    assert_eq!(
        next_wire(&mut h, now).as_deref(),
        Some("DISPLAYMODE 1920 1080\n")
    );
}

/// The readers of run.log (vm_bench.py resize(), verify-a8-window-resize.sh)
/// take a line that starts with BVDISPLAY or BVDPI: a verb's reply gives the
/// same one as the RUN line's did.
#[test]
fn run_log_gets_the_same_bvdisplay_and_bvdpi_lines_from_a_verb() {
    use super::super::harness_protocol::command_result_text;
    let lines = |text: &str| -> Vec<String> {
        text.lines()
            .map(|line| line.trim_end_matches('\r'))
            .filter(|line| line.starts_with("BVD"))
            .map(str::to_string)
            .collect()
    };
    let switch = desktop_switch_run_command(1920, 1080);
    let scale = desktop_scale_run_command(150);
    for (verb, run, line) in [
        ("DISPLAYMODE 1920 1080", &switch, SWITCHED),
        ("DISPLAYSCALE 150", &scale, SCALED),
    ] {
        let mut output = String::new();
        base64_encode_into(format!("{line}\r\n").as_bytes(), &mut output);
        let logged = command_result_text(verb, 0, &output);
        assert_eq!(lines(&logged), [line]);
        assert_eq!(lines(&logged), lines(&command_result_text(run, 0, &output)));
        assert!(
            logged.starts_with(&format!("BVAGENT CMD {verb} exit=0\n")),
            "{logged}"
        );
    }
}

/// bvagent.ps1 compiles the C# the RUN line carries, one concat! piece per line.
#[test]
fn the_agent_compiles_the_same_csharp_as_the_run_line() {
    let agent = include_str!("../../../../../scripts/win-assets/bvagent.ps1");
    for (verb, body) in [
        (MODE_VERB, super::super::desktop_switch::SCRIPT_BODY),
        (SCALE_VERB, super::super::desktop_scale::SCRIPT_BODY),
    ] {
        let csharp = body
            .strip_prefix("Add-Type -TypeDefinition '")
            .unwrap()
            .strip_suffix("';")
            .unwrap();
        let start = format!("    '{verb}' = @'\r\n");
        let (_, rest) = agent
            .split_once(&start)
            .unwrap_or_else(|| panic!("no {verb} source"));
        let (source, _) = rest.split_once("\r\n'@").unwrap();
        assert_eq!(source.replace("\r\n", ""), csharp, "{verb}");
    }
}

/// A switch or scale is safe to apply twice; one the restart cut runs again.
#[test]
fn a_display_verb_cut_by_an_agent_restart_runs_again() {
    let (mut h, now) = running();
    for command in ["DISPLAYMODE 1920 1080", "DISPLAYSCALE 150"] {
        h.queue.clear();
        h.in_flight = Some((ServiceReq::Ctl(command.to_string()), now));
        reply(&mut h, "READY BRIDGEVM v3-share2", now);
        let queued = h
            .queue
            .iter()
            .any(|req| matches!(req, ServiceReq::Ctl(c) if c == command));
        assert!(queued, "{command}");
    }
}

/// A display verb with no RUN form (a malformed control-file line) is answered
/// once, like any command, and never queued again.
#[test]
fn a_malformed_display_verb_is_not_retried() {
    let (mut h, now) = running();
    for command in [
        "DISPLAYMODE 1920",
        "DISPLAYMODE wide 1080",
        "DISPLAYSCALE 999",
    ] {
        h.queue.push_back(ServiceReq::Ctl(command.to_string()));
        assert_eq!(next_wire(&mut h, now), Some(format!("{command}\n")));
        reply(
            &mut h,
            &format!("ERR {} args", command.split(' ').next().unwrap()),
            now,
        );
        assert!(h.in_flight.is_none() && h.queue.is_empty(), "{command}");
        assert!(!h.display_verbs_refused, "{command}");
    }
}
