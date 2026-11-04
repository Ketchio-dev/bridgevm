use super::*;

/// The same fixture as scripts/test-bvagent-update.ps1: every file holds
/// "fixture <name>\r\n", and bvagent-update.ps1 is missing.
const FIXTURE_ID: &str = "287c4fd3fa27761d";

fn fixture_digests() -> Vec<(&'static str, Option<[u8; 32]>)> {
    AGENT_FILES
        .iter()
        .map(|&name| {
            let digest = (name != "bvagent-update.ps1")
                .then(|| Sha256::digest(format!("fixture {name}\r\n").as_bytes()).into());
            (name, digest)
        })
        .collect()
}

#[test]
fn agent_id_matches_the_agents_own_computation() {
    assert_eq!(agent_id(&fixture_digests()), FIXTURE_ID);
}

#[test]
fn agent_hashes_the_same_files_in_the_same_order() {
    let script = include_str!("../../../../../scripts/win-assets/bvagent.ps1");
    let list = script
        .lines()
        .find_map(|line| line.trim().strip_prefix("$files = @("))
        .and_then(|rest| rest.strip_suffix(')'))
        .expect("Get-BvAgentId's file list");
    let names: Vec<&str> = list.split(", ").map(|name| name.trim_matches('\'')).collect();
    assert_eq!(names, AGENT_FILES);
}

#[test]
fn ready_agent_id_reads_only_a_well_formed_id() {
    assert_eq!(ready_agent_id("BRIDGEVM v3-share2 agent=0123456789abcdef"), Some("0123456789abcdef"));
    for old in [
        "BRIDGEVM v3-share2",
        "BRIDGEVM",
        "BRIDGEVM v3-share2 agent=",
        "BRIDGEVM v3-share2 agent=0123456789ABCDEF",
        "BRIDGEVM v3-share2 agent=0123456789abcde",
        "BRIDGEVM v3-share2 agent=0123456789abcdef0",
        "BRIDGEVM v3-share2 agent=0123456789abcdeg",
    ] {
        assert_eq!(ready_agent_id(old), None, "{old}");
    }
    assert!(ready_has_file_verbs("BRIDGEVM v3-share2"));
    assert!(ready_has_file_verbs("BRIDGEVM v3-share2 agent=0123456789abcdef"));
    assert!(!ready_has_file_verbs("BRIDGEVM"));
}

#[test]
fn bundled_agent_loads_every_file_and_its_id() {
    let dir = std::env::temp_dir().join(format!("bv-agent-bundle-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    for name in AGENT_FILES {
        std::fs::write(dir.join(name), format!("fixture {name}\r\n")).unwrap();
    }
    let bundled = BundledAgent::load(&dir).unwrap();
    let mut digests = fixture_digests();
    digests[8].1 = Some(Sha256::digest(b"fixture bvagent-update.ps1\r\n").into());
    assert_eq!(bundled.id, agent_id(&digests));
    assert_eq!(bundled.files.len(), AGENT_FILES.len());
    assert_eq!(bundled.files[0].bytes, b"fixture bvagent.ps1\r\n");

    std::fs::remove_file(dir.join("bvagent-task.ps1")).unwrap();
    assert_eq!(BundledAgent::load(&dir).err().as_deref(), Some("missing bvagent-task.ps1"));
    std::fs::remove_dir_all(&dir).unwrap();
}

// --- The update state machine against a modelled guest -------------------

use crate::agent_console::agent_console_tests::tests::harness;
use sha2::{Digest, Sha256};
use std::collections::HashMap;

const CLIP_DIR: &str = r"C:\Users\u\AppData\Local\Temp\BridgeVM-clip";

const OLD_READY: &str = "BRIDGEVM v3-share2";

/// A temporary app copy of the agent; bvagent.ps1 spans three chunks.
struct Bundle(PathBuf);

impl Bundle {
    fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("bv-agent-update-{label}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in AGENT_FILES {
            let mut text = format!("fixture {name}\r\n");
            if name == "bvagent.ps1" {
                text.push_str(&"# padding\r\n".repeat(5_000));
            }
            std::fs::write(dir.join(name), text).unwrap();
        }
        Self(dir)
    }
    fn marker(&self) -> PathBuf {
        self.0.join("agent-update.json")
    }
    fn marker_state(&self) -> Option<String> {
        let value: serde_json::Value =
            serde_json::from_slice(&std::fs::read(self.marker()).ok()?).ok()?;
        value["state"].as_str().map(str::to_string)
    }
}

impl Drop for Bundle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// A resident service whose agent service started at `h.start`.
fn resident(bundle: &Bundle, session: &str) -> AgentConsoleHarness {
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.service = true;
    let bundled = BundledAgent::load(&bundle.0).unwrap();
    let mut update = AgentUpdate::new(bundled, Some(bundle.marker()), session.to_string());
    update.service_since = Some(h.start);
    h.agent_update = Some(update);
    h
}

fn phase(h: &AgentConsoleHarness) -> UpdatePhase {
    h.agent_update.as_ref().unwrap().phase
}

fn bundled_id(h: &AgentConsoleHarness) -> String {
    h.agent_update.as_ref().unwrap().bundled.id.clone()
}

/// Answers as bvagent.ps1 and bvagent-update.ps1 do, and keeps what arrives.
#[derive(Default)]
struct Guest {
    files: HashMap<String, Vec<u8>>,
    put: Option<(String, usize, Vec<u8>)>,
    /// The PowerShell of every RUN, decoded.
    scripts: Vec<String>,
    /// Every wire line's verb: RUN, PUTBEG, PUTCHUNK, PUTEND.
    verbs: Vec<String>,
    longest_run: usize,
    arm_reply: Option<String>,
    prepare_reply: Option<String>,
    confirm_reply: Option<String>,
    /// The image the next CLIPX announces, as guest.png.
    clip_image: Option<Vec<u8>>,
}

impl Guest {
    fn reply(&mut self, line: &str) -> Vec<String> {
        let line = line.trim_end();
        let (verb, rest) = line.split_once(' ').unwrap_or((line, ""));
        self.verbs.push(verb.to_string());
        match verb {
            "RUN" => {
                let command = String::from_utf8(base64_decode(rest).unwrap()).unwrap();
                self.longest_run = self.longest_run.max(command.len());
                let encoded = command.rsplit(' ').next().unwrap();
                let utf16 = base64_decode(encoded).unwrap();
                let units: Vec<u16> = utf16.chunks(2).map(|p| u16::from_le_bytes([p[0], p[1]])).collect();
                let script = String::from_utf16(&units).unwrap();
                let out = if script.contains("'BVUPDATE ready'") {
                    self.prepare_reply.clone().unwrap_or("BVUPDATE ready\r\n".to_string())
                } else if script.contains(" -Arm ") {
                    self.arm_reply.clone().unwrap_or("BVUPDATE armed id=x\r\n".to_string())
                } else if script.contains(" -Confirm ") {
                    self.confirm_reply.clone().unwrap_or("BVUPDATE confirmed id=x\r\n".to_string())
                } else {
                    String::new()
                };
                self.scripts.push(script);
                let code = if out.contains("result=failed") { 2 } else { 0 };
                vec![format!("OUT {code} {}", base64_encode(out.as_bytes()))]
            }
            "PUTBEG" => {
                let mut parts = rest.split(' ');
                let path = String::from_utf8(base64_decode(parts.next().unwrap()).unwrap()).unwrap();
                let total = parts.next().unwrap().parse().unwrap();
                self.put = Some((path, total, Vec::new()));
                vec!["OK PUTBEG".into()]
            }
            "PUTCHUNK" => {
                let (seq, b64) = rest.split_once(' ').unwrap();
                self.put.as_mut().unwrap().2.extend(base64_decode(b64).unwrap());
                vec![format!("OK PUTCHUNK {seq}")]
            }
            "PUTEND" => {
                let (path, total, bytes) = self.put.take().unwrap();
                assert_eq!(bytes.len(), total);
                let reply = format!("PUTOK {} {}", base64_encode(path.as_bytes()), bytes.len());
                self.files.insert(path, bytes);
                vec![reply]
            }
            "PING" => vec!["PONG".into()],
            "CLIPX" => {
                let descriptor = match self.clip_image.clone() {
                    Some(bytes) => {
                        let path = format!(r"{CLIP_DIR}\guest.png");
                        let sha: [u8; 32] = Sha256::digest(&bytes).into();
                        let line = format!("1 1 {} {} {}", bytes.len(), hex_lower(&sha), base64_encode(path.as_bytes()));
                        self.files.insert(path, bytes);
                        line
                    }
                    None => "0 0 0 - -".to_string(),
                };
                vec![format!("CLIPX {descriptor} {}", base64_encode(b"t"))]
            }
            "CLIPSETIMG" => vec!["OK CLIPSETIMG 1 1".into()],
            "GET" => {
                let path = String::from_utf8(base64_decode(rest).unwrap()).unwrap();
                let bytes = self.files[&path].clone();
                vec![
                    format!("GETBEG {rest} {} 1", bytes.len()),
                    format!("GETCHUNK 0 {}", base64_encode(&bytes)),
                    "GETEND 1".to_string(),
                ]
            }
            _ => vec![format!("OUT 255 {}", base64_encode(b"unknown token"))],
        }
    }
}

/// Pumps the service and the guest until the wire is idle; drops before the
/// update, in service_enqueue's order.
fn run(h: &mut AgentConsoleHarness, guest: &mut Guest, now: Instant) {
    for _ in 0..200 {
        h.enqueue_file_drop(now);
        h.enqueue_agent_update(now);
        let mut wire = Vec::new();
        h.service_pump_with_sender(now, &mut |bytes| wire.push(String::from_utf8(bytes.to_vec()).unwrap()));
        if wire.is_empty() {
            return;
        }
        answer(h, guest, wire, now);
    }
    panic!("the update never went idle");
}

/// Hands every wire line to the guest and every reply back, in lockstep.
fn answer(h: &mut AgentConsoleHarness, guest: &mut Guest, mut wire: Vec<String>, now: Instant) {
    while let Some(line) = wire.pop() {
        for reply in guest.reply(&line) {
            h.handle_service_reply_with_sender(&reply, now, &mut |bytes| {
                wire.push(String::from_utf8(bytes.to_vec()).unwrap())
            });
        }
    }
}

#[test]
fn an_older_agent_gets_every_file_then_the_new_one_is_confirmed() {
    let bundle = Bundle::new("happy");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    run(&mut h, &mut guest, t0);
    assert_eq!(phase(&h), UpdatePhase::Idle, "nothing before the settle time");
    assert!(guest.verbs.is_empty());

    let t1 = t0 + SETTLE;
    run(&mut h, &mut guest, t1);
    assert!(matches!(phase(&h), UpdatePhase::Armed { late: false, .. }));
    assert_eq!(bundle.marker_state().as_deref(), Some("armed"));
    let id = bundled_id(&h);
    for name in AGENT_FILES {
        let path = format!(r"C:\Program Files\BridgeVM\Agent\{id}.new\{name}");
        assert_eq!(guest.files[&path], std::fs::read(bundle.0.join(name)).unwrap(), "{name}");
    }
    let mut expected = vec!["RUN"];
    for name in AGENT_FILES {
        let chunks = std::fs::metadata(bundle.0.join(name)).unwrap().len().div_ceil(SHARE_PUT_CHUNK_BYTES as u64);
        expected.push("PUTBEG");
        expected.extend(std::iter::repeat_n("PUTCHUNK", chunks as usize));
        expected.push("PUTEND");
    }
    expected.push("RUN");
    assert_eq!(guest.verbs, expected);
    assert!(guest.longest_run <= crate::agent_console::desktop_switch::MAX_RUN_COMMAND_CHARS);
    let prepare = &guest.scripts[0];
    assert!(prepare.contains("'O:S-1-5-32-544D:PAI(A;OICI;FA;;;S-1-5-18)(A;OICI;FA;;;S-1-5-32-544)'"));
    assert!(prepare.contains(&format!("'{id}.new'")));
    let arm = &guest.scripts[1];
    for file in &h.agent_update.as_ref().unwrap().bundled.files {
        assert!(arm.contains(&format!("{}={}", file.name, hex_lower(&file.sha256))), "{}", file.name);
    }

    // The guard restarted the agent: the new one is confirmed.
    let t2 = t1 + Duration::from_secs(8);
    h.handle_service_reply_with_sender(&format!("READY BRIDGEVM v3-share2 agent={id}"), t2, &mut |_| {});
    run(&mut h, &mut guest, t2);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("ok"));
    assert!(guest.scripts[2].contains(&format!(r"\{id}\bvagent-update.ps1' -Confirm -Id '{id}'")));
    assert_eq!(guest.verbs.len(), expected.len() + 1);
}

#[test]
fn the_bundled_agent_is_left_alone() {
    let bundle = Bundle::new("current");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    let id = bundled_id(&h);
    h.note_agent_ready(&format!("BRIDGEVM v3-share2 agent={id}"), t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert!(guest.verbs.is_empty());
    assert_eq!(bundle.marker_state(), None);
}

#[test]
fn each_app_session_tries_once() {
    let bundle = Bundle::new("session");
    let mut guest = Guest::default();
    guest.arm_reply = Some("BVUPDATE result=failed detail=parse bvagent.ps1 12:3\r\n".into());
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("failed"));
    assert_eq!(guest.scripts.len(), 2, "Prepare and a refused Arm, nothing after it");

    // A guest reboot in the same session recreates the probe: no second try.
    let mut again = resident(&bundle, "session-1");
    let start = again.start;
    again.note_agent_ready(OLD_READY, start);
    run(&mut again, &mut guest, start + SETTLE);
    assert_eq!(guest.scripts.len(), 2);

    // The next app session tries again.
    let mut next = resident(&bundle, "session-2");
    let start = next.start;
    next.note_agent_ready(OLD_READY, start);
    run(&mut next, &mut guest, start + SETTLE);
    assert_eq!(guest.scripts.len(), 4);
}

#[test]
fn the_old_agent_back_after_arm_is_a_rollback() {
    let bundle = Bundle::new("rollback");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    assert!(matches!(phase(&h), UpdatePhase::Armed { .. }));
    h.handle_service_reply_with_sender(&format!("READY {OLD_READY}"), t0 + SETTLE + Duration::from_secs(185), &mut |_| {});
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("rolled-back"));
    run(&mut h, &mut guest, t0 + SETTLE + Duration::from_secs(186));
    assert_eq!(guest.scripts.len(), 2, "no Confirm for the old agent");
}

#[test]
fn no_ready_by_the_deadline_is_reported_and_a_late_one_still_confirms() {
    let bundle = Bundle::new("late");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    let late = t0 + SETTLE + READY_DEADLINE;
    run(&mut h, &mut guest, late);
    assert!(matches!(phase(&h), UpdatePhase::Armed { late: true, .. }));
    assert_eq!(bundle.marker_state().as_deref(), Some("failed"));
    let id = bundled_id(&h);
    h.handle_service_reply_with_sender(&format!("READY BRIDGEVM v3-share2 agent={id}"), late, &mut |_| {});
    run(&mut h, &mut guest, late);
    assert_eq!(bundle.marker_state().as_deref(), Some("ok"));
}

/// The guest's update=pending decides, whatever the host recorded: each of
/// these left the guard to roll a good update back 180 s later.
#[test]
fn a_pending_update_is_confirmed_whatever_the_marker_says() {
    for case in ["reboot after the deadline", "arm failed after the guard", "unwritable marker", "new session"] {
        let bundle = Bundle::new(&format!("pending-{}", case.replace(' ', "-")));
        let mut first = resident(&bundle, "session-1");
        let mut guest = Guest::default();
        if case == "arm failed after the guard" {
            guest.arm_reply = Some("BVUPDATE result=failed detail=Start-ScheduledTask failed\r\n".into());
        }
        let t0 = first.start;
        first.note_agent_ready(OLD_READY, t0);
        run(&mut first, &mut guest, t0 + SETTLE);
        let id = bundled_id(&first);
        let mut h = match case {
            "reboot after the deadline" => {
                first.enqueue_agent_update(t0 + SETTLE + READY_DEADLINE);
                assert_eq!(bundle.marker_state().as_deref(), Some("failed"));
                resident(&bundle, "session-1")
            }
            "arm failed after the guard" => {
                assert_eq!(phase(&first), UpdatePhase::Done);
                assert_eq!(bundle.marker_state().as_deref(), Some("failed"));
                first
            }
            "unwritable marker" => {
                let mut next = resident(&bundle, "session-1");
                next.agent_update.as_mut().unwrap().marker = None;
                next
            }
            _ => resident(&bundle, "session-2"),
        };
        let now = t0 + SETTLE + Duration::from_secs(8);
        let ready = format!("READY BRIDGEVM v3-share2 agent={id} update=pending");
        h.handle_service_reply_with_sender(&ready, now, &mut |_| {});
        run(&mut h, &mut guest, now);
        assert_eq!(phase(&h), UpdatePhase::Done, "{case}");
        assert!(guest.scripts.last().unwrap().contains(" -Confirm "), "{case}: no Confirm");
        if case != "unwritable marker" {
            assert_eq!(bundle.marker_state().as_deref(), Some("ok"), "{case}");
        }
    }
}

#[test]
fn a_bundled_agent_with_nothing_pending_is_not_confirmed() {
    let bundle = Bundle::new("not-pending");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let id = bundled_id(&h);
    let t0 = h.start;
    h.note_agent_ready(&format!("BRIDGEVM v3-share2 agent={id}"), t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert!(guest.verbs.is_empty(), "{:?}", guest.verbs);
}

/// The guard decides; Confirm reports its verdict.
#[test]
fn the_guards_verdict_ends_the_confirm() {
    for (reply, marker) in [("BVUPDATE rolled-back id=x", "rolled-back"), ("BVUPDATE nothing-pending id=x", "armed")] {
        let bundle = Bundle::new(&format!("verdict-{marker}"));
        let mut h = resident(&bundle, "session-1");
        let mut guest = Guest { confirm_reply: Some(format!("{reply}\r\n")), ..Guest::default() };
        let t0 = h.start;
        h.note_agent_ready(OLD_READY, t0);
        run(&mut h, &mut guest, t0 + SETTLE);
        let id = bundled_id(&h);
        let now = t0 + SETTLE + Duration::from_secs(8);
        h.handle_service_reply_with_sender(&format!("READY BRIDGEVM v3-share2 agent={id} update=pending"), now, &mut |_| {});
        run(&mut h, &mut guest, now);
        assert_eq!(phase(&h), UpdatePhase::Done, "{reply}");
        assert_eq!(bundle.marker_state().as_deref(), Some(marker), "{reply}");
    }
}

#[test]
fn an_agent_without_a_task_is_left_alone_with_no_later_traffic() {
    let bundle = Bundle::new("no-task");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest { prepare_reply: Some("BVUPDATE skipped no-task\r\n".into()), ..Guest::default() };
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(guest.verbs, ["RUN"], "Prepare only");
    assert!(guest.scripts[0].contains("'BVUPDATE skipped no-task'"));
    assert_eq!(bundle.marker_state().as_deref(), Some("skipped"));

    // A later app session sends nothing for this version.
    let mut next = resident(&bundle, "session-2");
    let mut quiet = Guest::default();
    let t1 = next.start;
    next.note_agent_ready(OLD_READY, t1);
    run(&mut next, &mut quiet, t1 + SETTLE);
    assert_eq!(phase(&next), UpdatePhase::Done);
    assert!(quiet.verbs.is_empty(), "{:?}", quiet.verbs);
}

fn stop_through_the_control_file(h: &mut AgentConsoleHarness) {
    let ctl = std::env::temp_dir().join(format!("bv-agent-update-ctl-{}", std::process::id()));
    std::fs::write(&ctl, "shutdown.exe /s /t 0\n").unwrap();
    h.ctl_path = Some(ctl.display().to_string());
    h.ingest_ctl();
    std::fs::remove_file(&ctl).unwrap();
}

#[test]
fn a_shutdown_before_the_arm_stops_the_update_and_goes_first() {
    let bundle = Bundle::new("stop");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    h.enqueue_agent_update(t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Staging);
    // The app's Stop arrives through the control file before Prepare went out:
    // the shutdown is next, and no update step follows it.
    stop_through_the_control_file(&mut h);
    assert!(matches!(h.queue.front(), Some(ServiceReq::Ctl(c)) if c.starts_with("shutdown.exe")), "the shutdown goes first");
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("stopped"));
    h.queue.clear();
    run(&mut h, &mut guest, t0 + SETTLE);
    assert!(guest.verbs.is_empty(), "{:?}", guest.verbs);

    // Mid-transfer: the file on the wire finishes, then nothing more.
    let bundle = Bundle::new("stop-put");
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    h.enqueue_agent_update(t0 + SETTLE);
    let mut wire = Vec::new();
    h.service_pump_with_sender(t0 + SETTLE, &mut |b| wire.push(String::from_utf8(b.to_vec()).unwrap()));
    h.handle_service_reply_with_sender(&format!("OUT 0 {}", base64_encode(b"BVUPDATE ready\r\n")), t0 + SETTLE, &mut |_| {});
    h.service_pump_with_sender(t0 + SETTLE, &mut |b| wire.push(String::from_utf8(b.to_vec()).unwrap()));
    assert!(wire.last().unwrap().starts_with("PUTBEG "), "{wire:?}");
    stop_through_the_control_file(&mut h);
    assert!(!h.queue.iter().any(is_update_req));
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("stopped"));
}

#[test]
fn a_put_the_agent_cannot_take_fails_without_an_arm() {
    let bundle = Bundle::new("put");
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    h.enqueue_agent_update(t0 + SETTLE);
    let mut wire = Vec::new();
    h.service_pump_with_sender(t0 + SETTLE, &mut |b| wire.push(String::from_utf8(b.to_vec()).unwrap()));
    h.handle_service_reply_with_sender(&format!("OUT 0 {}", base64_encode(b"BVUPDATE ready\r\n")), t0 + SETTLE, &mut |_| {});
    h.service_pump_with_sender(t0 + SETTLE, &mut |b| wire.push(String::from_utf8(b.to_vec()).unwrap()));
    assert!(wire[1].starts_with("PUTBEG "));
    h.handle_service_reply_with_sender(&format!("OUT 255 {}", base64_encode(b"unknown token: PUTBEG")), t0 + SETTLE, &mut |_| {});
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("failed"));
    assert!(h.queue.is_empty() && h.in_flight.is_none());
}

#[test]
fn an_agent_without_file_verbs_is_skipped() {
    let bundle = Bundle::new("old");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready("BRIDGEVM", t0);
    run(&mut h, &mut guest, t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert!(guest.verbs.is_empty());
}

#[test]
fn an_agent_restart_mid_put_ends_the_attempt() {
    let bundle = Bundle::new("restart");
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    h.enqueue_agent_update(t0 + SETTLE);
    h.service_pump_with_sender(t0 + SETTLE, &mut |_| {});
    h.handle_service_reply_with_sender(&format!("OUT 0 {}", base64_encode(b"BVUPDATE ready\r\n")), t0 + SETTLE, &mut |_| {});
    h.service_pump_with_sender(t0 + SETTLE, &mut |_| {});
    assert!(matches!(h.in_flight, Some((ServiceReq::AgentUpdate(UpdateStep::Put { .. }), _))));
    h.handle_service_reply_with_sender(&format!("READY {OLD_READY}"), t0 + SETTLE, &mut |_| {});
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert_eq!(bundle.marker_state().as_deref(), Some("failed"));
}

#[test]
fn shutdown_commands_are_recognised() {
    let bundle = Bundle::new("shutdown");
    let mut update = AgentUpdate::new(BundledAgent::load(&bundle.0).unwrap(), None, String::new());
    for command in ["whoami", "shutdownx /s", "echo shutdown.exe"] {
        update.note_ctl_command(command);
        assert!(!update.stop_requested, "{command}");
    }
    for command in ["shutdown.exe /s /t 0", "shutdown.exe /p /f", "SHUTDOWN /r", r"C:\Windows\System32\shutdown.exe /s"] {
        update.stop_requested = false;
        update.note_ctl_command(command);
        assert!(update.stop_requested, "{command}");
    }
}

fn drop_staging(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bv-agent-update-drop-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("a.txt"), b"dropped").unwrap();
    dir
}

fn submit_drop(h: &mut AgentConsoleHarness, id: &str, root: &Path) {
    let b64 = base64_encode(root.to_str().unwrap().as_bytes());
    h.drops.submit(&format!("{id} {b64}"));
}

#[test]
fn a_running_drop_goes_before_the_update() {
    let bundle = Bundle::new("drop-holds");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);

    // A drop step on the wire holds the begin.
    h.queue.push_back(ServiceReq::Drop(DropReq::Caps));
    h.enqueue_agent_update(t0 + SETTLE);
    assert_eq!(phase(&h), UpdatePhase::Idle);
    h.queue.clear();

    // A drop dropped before the update is due starts first; the update
    // begins once it ends (this guest has no drop verbs, so after DROPCAPS).
    let root = drop_staging("holds");
    submit_drop(&mut h, "job-1", &root);
    run(&mut h, &mut guest, t0 + SETTLE);
    let caps = guest.verbs.iter().position(|verb| verb == "DROPCAPS").expect("the drop ran");
    let prepare = guest.verbs.iter().position(|verb| verb == "RUN").expect("the update began");
    assert!(caps < prepare, "{:?}", guest.verbs);
    assert!(matches!(phase(&h), UpdatePhase::Armed { late: false, .. }), "{:?}", phase(&h));
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_waiting_drop_lets_the_update_arm_and_starts_after_it() {
    let bundle = Bundle::new("drop-arm");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    let t1 = t0 + SETTLE;
    h.enqueue_agent_update(t1);
    assert_eq!(phase(&h), UpdatePhase::Staging);

    // A drop dropped onto the window while the files move waits, and the
    // update still arms: the Arm counted waiting drops, which cannot start
    // during the update, so each waited on the other until a Stop.
    let root = drop_staging("arm");
    submit_drop(&mut h, "job-2", &root);
    run(&mut h, &mut guest, t1);
    assert!(matches!(phase(&h), UpdatePhase::Armed { late: false, .. }), "{:?}", phase(&h));
    assert!(!guest.verbs.iter().any(|verb| verb.starts_with("DROP")), "no drop step during the update");

    // The guard restarted the agent and the new one is confirmed; the waiting drop starts.
    let id = bundled_id(&h);
    let t2 = t1 + Duration::from_secs(8);
    h.handle_service_reply_with_sender(&format!("READY BRIDGEVM v3-share2 agent={id}"), t2, &mut |_| {});
    run(&mut h, &mut guest, t2);
    assert_eq!(phase(&h), UpdatePhase::Done);
    let confirm = guest.verbs.iter().rposition(|verb| verb == "RUN").unwrap();
    assert_eq!(guest.verbs.get(confirm + 1).map(String::as_str), Some("DROPCAPS"), "{:?}", guest.verbs);
    std::fs::remove_dir_all(&root).unwrap();
}

#[test]
fn a_late_update_no_longer_holds_drops() {
    let bundle = Bundle::new("drop-late");
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    h.agent_update.as_mut().unwrap().phase = UpdatePhase::Armed { since: t0, late: false };
    assert!(h.agent_update_busy());
    h.agent_update.as_mut().unwrap().phase = UpdatePhase::Armed { since: t0, late: true };
    assert!(!h.agent_update_busy());
}

/// The guard restarts the agent at any time: a DNS flush (or a desktop mode or
/// scale) on the wire then runs again; other commands do not run twice.
#[test]
fn a_dns_flush_cut_by_an_agent_restart_runs_again() {
    let bundle = Bundle::new("ctl-restart");
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    for (command, again) in [("ipconfig /flushdns", true), ("whoami /all", false)] {
        h.queue.clear();
        h.in_flight = Some((ServiceReq::Ctl(command.to_string()), t0));
        h.resync_after_agent_restart("BRIDGEVM v3-share2", t0);
        let queued = h.queue.iter().any(|req| matches!(req, ServiceReq::Ctl(c) if c == command));
        assert_eq!(queued, again, "{command}");
    }
}

/// Prepare clears the staging folders: an overdue one is still running, and a
/// second one would race it.
#[test]
fn an_overdue_prepare_is_not_sent_twice() {
    let bundle = Bundle::new("prepare-overdue");
    let mut h = resident(&bundle, "session-1");
    let t0 = h.start;
    h.note_agent_ready(OLD_READY, t0);
    h.enqueue_agent_update(t0 + SETTLE);
    let mut wire = Vec::new();
    h.service_pump_with_sender(t0 + SETTLE, &mut |b| wire.push(String::from_utf8(b.to_vec()).unwrap()));
    assert_eq!(wire.len(), 1);
    for beat in 1..=5 {
        h.service_pump_with_sender(t0 + SETTLE + SERVICE_OVERDUE_INTERVAL * beat, &mut |b| {
            wire.push(String::from_utf8(b.to_vec()).unwrap())
        });
    }
    assert_eq!(wire.len(), 1, "Prepare went again: {wire:?}");
    assert!(matches!(h.in_flight, Some((ServiceReq::AgentUpdate(UpdateStep::Prepare), _))));
}

/// A 1x1 PNG as the clipboard sync admits it; `seed` tells copies apart.
fn clip_png(seed: u8) -> Vec<u8> {
    let mut png = vec![0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1a, b'\n', 0, 0, 0, 13];
    png.extend_from_slice(b"IHDR");
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&1u32.to_be_bytes());
    png.extend_from_slice(&[8, 6, 0, 0, 0, 0, 0, 0, 0]);
    png.resize(png.len() + 64, seed);
    png
}

fn copy_mac_image(h: &mut AgentConsoleHarness, seed: u8, now: Instant) {
    let png = crate::agent_console::host_pasteboard::admit_png(clip_png(seed)).unwrap();
    h.enqueue_host_image(None, png, now);
}

/// Through the real pump: a clipboard image on the move holds the update, and
/// the update's file stream and agent switch hold a Mac image, while the text
/// poll keeps running.
#[test]
fn clipboard_images_and_the_update_wait_for_each_other() {
    let bundle = Bundle::new("clip-images");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    let caps = format!("CLIPCAPS IMAGE_PNG {}", base64_encode(CLIP_DIR.as_bytes()));
    h.handle_clip_caps_reply(&caps, t0);
    h.note_agent_ready(OLD_READY, t0);
    let t1 = t0 + SETTLE;

    // A Mac image on the wire: the update does not begin under it.
    copy_mac_image(&mut h, 1, t1);
    let mut wire = Vec::new();
    h.service_pump_with_sender(t1, &mut |b| wire.push(String::from_utf8(b.to_vec()).unwrap()));
    assert!(wire[0].starts_with("PUTBEG "), "{wire:?}");
    h.enqueue_agent_update(t1);
    assert_eq!(phase(&h), UpdatePhase::Idle, "the update began under an image transfer");
    answer(&mut h, &mut guest, wire, t1);
    assert_eq!(guest.verbs.last().map(String::as_str), Some("CLIPSETIMG"));

    // A Mac image copied once the update began waits out the file stream,
    // then goes before the Arm; the text poll runs too.
    guest.verbs.clear();
    h.enqueue_agent_update(t1);
    assert_eq!(phase(&h), UpdatePhase::Staging);
    copy_mac_image(&mut h, 2, t1);
    h.queue.push_back(ServiceReq::ClipPoll);
    run(&mut h, &mut guest, t1);
    assert!(matches!(phase(&h), UpdatePhase::Armed { late: false, .. }));
    let verbs = &guest.verbs;
    let set = verbs.iter().position(|v| v == "CLIPSETIMG").expect("the Mac image never went");
    let ends = verbs[..set].iter().filter(|v| *v == "PUTEND").count();
    assert_eq!(ends, AGENT_FILES.len() + 1, "the image cut into the file stream: {verbs:?}");
    let runs: Vec<usize> = (0..verbs.len()).filter(|&i| verbs[i] == "RUN").collect();
    assert!(runs.len() == 2 && runs[1] > set, "the Arm went before the image: {verbs:?}");
    assert!(verbs.iter().any(|v| v == "CLIPX"), "the text poll stopped: {verbs:?}");

    // During the switch a Mac image waits; the text poll still runs. Poll
    // replies are dropped while a Mac image is pending (the newer Mac copy
    // wins), so the guest direction has its own test below.
    guest.verbs.clear();
    copy_mac_image(&mut h, 3, t1);
    h.queue.push_back(ServiceReq::ClipPoll);
    run(&mut h, &mut guest, t1);
    assert_eq!(guest.verbs, ["CLIPX"], "an image moved during the switch");

    // The new agent is confirmed: the Mac image goes.
    let t2 = t1 + Duration::from_secs(8);
    confirm_new_agent(&mut h, t2);
    guest.verbs.clear();
    run(&mut h, &mut guest, t2);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert!(guest.verbs.iter().any(|v| v == "CLIPSETIMG"), "{:?}", guest.verbs);
}

/// The guard restarted the agent: the new one's READY.
fn confirm_new_agent(h: &mut AgentConsoleHarness, now: Instant) {
    let ready = format!("READY BRIDGEVM v3-share2 agent={} update=pending", bundled_id(h));
    h.handle_service_reply_with_sender(&ready, now, &mut |_| {});
}

/// A guest image announced while the guard switches agents is not queued for
/// fetching: the polls go on and rewrite guest.png, so a queued fetch would
/// find a later image after the update and refuse it as a mismatch. The first
/// poll after the update announces the image again and fetches it, once, as
/// it is then.
#[test]
fn a_guest_image_announced_during_the_switch_is_fetched_after_it() {
    let bundle = Bundle::new("clip-guest-image");
    let mut h = resident(&bundle, "session-1");
    let mut guest = Guest::default();
    let t0 = h.start;
    let caps = format!("CLIPCAPS IMAGE_PNG {}", base64_encode(CLIP_DIR.as_bytes()));
    h.handle_clip_caps_reply(&caps, t0);
    h.note_agent_ready(OLD_READY, t0);
    let t1 = t0 + SETTLE;
    // A first poll learns the guest clipboard; the update runs to Armed.
    h.queue.push_back(ServiceReq::ClipPoll);
    run(&mut h, &mut guest, t1);
    assert!(matches!(phase(&h), UpdatePhase::Armed { late: false, .. }));

    // Two guest copies during the switch, no Mac copy pending: both polled,
    // neither fetched nor queued.
    guest.verbs.clear();
    for seed in [9, 10] {
        guest.clip_image = Some(clip_png(seed));
        h.queue.push_back(ServiceReq::ClipPoll);
        run(&mut h, &mut guest, t1);
    }
    assert_eq!(guest.verbs, ["CLIPX", "CLIPX"]);
    let queued = h.queue.iter().any(|r| matches!(r, ServiceReq::ClipImagePull { .. }));
    assert!(!queued, "a guest image fetch waits behind the switch");

    // The update ends: no fetch until the next poll, which fetches the guest's
    // image as it is now.
    let t2 = t1 + Duration::from_secs(8);
    confirm_new_agent(&mut h, t2);
    guest.verbs.clear();
    run(&mut h, &mut guest, t2);
    assert_eq!(phase(&h), UpdatePhase::Done);
    assert!(!guest.verbs.iter().any(|v| v == "GET"), "fetched before a poll: {:?}", guest.verbs);
    h.queue.push_back(ServiceReq::ClipPoll);
    run(&mut h, &mut guest, t2);
    assert_eq!(guest.verbs.iter().filter(|v| *v == "GET").count(), 1, "{:?}", guest.verbs);
    let digest: [u8; 32] = Sha256::digest(clip_png(10)).into();
    assert_eq!(h.last_synced.png.as_ref().map(|png| png.digest), Some(digest));
}
