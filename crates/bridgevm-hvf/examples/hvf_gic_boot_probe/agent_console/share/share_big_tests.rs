//! Large guest files over GETRANGE: reassembled range by range, published only complete, never
//! when the file changed meanwhile, abandoned without a trace when the agent restarts, with
//! foreground requests between ranges, and refused when the Mac lacks the space.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

const ROOT: &str = "C:\\share";
const TIME: &str = "2026-11-09T01:02:03.0000000Z";

struct Rig {
    h: AgentConsoleHarness,
    dir: PathBuf,
    now: Instant,
}

impl Drop for Rig {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn rig(label: &str, max_kb: u64) -> Rig {
    let dir = std::env::temp_dir().join(format!("bvagent-big-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let now = Instant::now();
    let mut h = harness();
    h.service = true;
    h.state = AgentConsoleState::Service;
    h.share_big = true;
    h.last_send = Some(now);
    let mut share = ShareState::new(
        ShareSync::new(max_kb),
        dir.clone(),
        ROOT.into(),
        Duration::from_secs(3600),
    );
    share.last_poll = Some(now);
    h.share = Some(share);
    Rig { h, dir, now }
}

impl Rig {
    /// The guest lists `name` at `size` and `mtime`; the listing's requests are queued.
    fn list(&mut self, name: &str, size: u64, mtime: &str) {
        let listing = format!("{name}|{size}|0|{mtime}\n");
        self.h.in_flight = Some((ServiceReq::ShareLs, self.now));
        let line = format!("LSOK {}", base64_encode(listing.as_bytes()));
        self.reply(&line);
    }
    fn reply(&mut self, line: &str) {
        self.h.handle_service_reply(line, None, None, self.now);
    }
    /// One service step: the writer's answers, then at most one request on the wire.
    fn step(&mut self) -> Option<String> {
        let mut sent = Vec::new();
        self.h.advance_big_share(self.now);
        self.h
            .service_pump_with_sender(self.now, &mut |bytes| sent.push(String::from_utf8_lossy(bytes).into_owned()));
        sent.pop()
    }
    /// Takes the writer thread's answers until `done`; sends nothing.
    fn wait(&mut self, mut done: impl FnMut(&mut Self) -> bool) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while !done(self) {
            assert!(Instant::now() < deadline, "timed out");
            self.h.advance_big_share(self.now);
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }
}

/// The agent's GETRANGE reply for `data` (bvagent.ps1: 768 KiB chunks), stat as given.
fn range_reply(line: &str, data: &[u8], size: u64, mtime: &str) -> Vec<String> {
    let mut f = line.trim_end().split(' ');
    assert_eq!(f.next(), Some("GETRANGE"), "{line}");
    let path = f.next().unwrap().to_string();
    let offset: usize = f.next().unwrap().parse().unwrap();
    let want: usize = f.next().unwrap().parse().unwrap();
    let end = (offset + want).min(data.len());
    let bytes = &data[offset.min(end)..end];
    let chunks: Vec<&[u8]> = bytes.chunks(786_432).collect();
    let mut out = vec![format!(
        "GETBEG {path} {} {} {offset} {size} {mtime}",
        bytes.len(),
        chunks.len()
    )];
    for (seq, chunk) in chunks.iter().enumerate() {
        out.push(format!("GETCHUNK {seq} {}", base64_encode(chunk)));
    }
    out.push(format!("GETEND {}", chunks.len()));
    out
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 31 + i / 7) as u8).collect()
}

fn is_range(line: &Option<String>) -> bool {
    line.as_deref().is_some_and(|line| line.starts_with("GETRANGE "))
}

#[test]
fn a_large_guest_file_is_reassembled_range_by_range_and_appears_only_complete() {
    let mut rig = rig("whole", 1 << 30);
    let bytes = data(2 * SHARE_RANGE_BYTES as usize + 12_345);
    let size = bytes.len() as u64;
    rig.list("video.mp4", size, TIME);
    let mut ranges = 0;
    while let Some(line) = rig.step().filter(|line| line.starts_with("GETRANGE ")) {
        let offset: u64 = line.split(' ').nth(2).unwrap().parse().unwrap();
        assert_eq!(offset, ranges * SHARE_RANGE_BYTES, "ranges go in order");
        for reply in range_reply(&line, &bytes, size, TIME) {
            rig.reply(&reply);
        }
        ranges += 1;
        assert!(
            !rig.dir.join("video.mp4").exists(),
            "the file appeared at its name before it was complete"
        );
        // The writer catches up before the next range: memory stays near a range or two.
        rig.wait(|rig| writer_backlog(rig) <= SHARE_RANGE_BYTES);
    }
    assert_eq!(ranges, 3);
    rig.wait(|rig| !rig.h.big_share_active());
    assert_eq!(std::fs::read(rig.dir.join("video.mp4")).unwrap(), bytes);
    assert_eq!(rig.files(), ["video.mp4"], "a temporary was left behind");
    // Recorded as synced: the same listing fetches nothing again.
    rig.list("video.mp4", size, TIME);
    assert!(rig.h.queue.is_empty(), "{:?}", rig.h.queue.iter().map(req_kind).collect::<Vec<_>>());
}

fn writer_backlog(rig: &mut Rig) -> u64 {
    rig.h.big_get().map_or(0, |get| get.writer.backlog)
}

#[test]
fn a_guest_file_changed_mid_copy_is_dropped_and_fetched_again() {
    let mut rig = rig("changed", 1 << 30);
    let bytes = data(2 * SHARE_RANGE_BYTES as usize + 1);
    let size = bytes.len() as u64;
    rig.list("doc.bin", size, TIME);
    let first = rig.step();
    for reply in range_reply(first.as_deref().unwrap(), &bytes, size, TIME) {
        rig.reply(&reply);
    }
    let second = rig.step();
    assert!(is_range(&second));
    // Windows wrote the file after the first range: its time moved.
    for reply in range_reply(second.as_deref().unwrap(), &bytes, size, "2026-11-09T01:02:09.0000000Z") {
        rig.reply(&reply);
    }
    assert!(!rig.h.big_share_active(), "the copy went on");
    assert!(rig.h.in_flight.is_none(), "the range's GETEND left the wire waiting");
    rig.wait(|rig| rig.files().is_empty());
    // The next listing fetches the file again.
    rig.list("doc.bin", size, "2026-11-09T01:02:09.0000000Z");
    assert!(is_range(&rig.step()));
}

#[test]
fn a_first_range_that_differs_from_the_listing_starts_no_copy() {
    let mut rig = rig("stale", 1 << 30);
    let bytes = data(SHARE_RANGE_BYTES as usize + 10);
    rig.list("grow.log", bytes.len() as u64 - 5, TIME);
    let first = rig.step();
    for reply in range_reply(first.as_deref().unwrap(), &bytes, bytes.len() as u64, TIME) {
        rig.reply(&reply);
    }
    assert!(!rig.h.big_share_active());
    assert!(rig.h.in_flight.is_none());
    assert!(rig.files().is_empty(), "{:?}", rig.files());
}

#[test]
fn an_agent_restart_mid_copy_leaves_neither_the_file_nor_its_temporary() {
    let mut rig = rig("restart", 1 << 30);
    let bytes = data(2 * SHARE_RANGE_BYTES as usize);
    let size = bytes.len() as u64;
    rig.list("big.iso", size, TIME);
    let first = rig.step();
    for reply in range_reply(first.as_deref().unwrap(), &bytes, size, TIME) {
        rig.reply(&reply);
    }
    let second = rig.step();
    assert!(is_range(&second));
    // Half of the second range, then the agent comes back fresh.
    let replies = range_reply(second.as_deref().unwrap(), &bytes, size, TIME);
    for reply in &replies[..replies.len() / 2] {
        rig.reply(reply);
    }
    rig.reply("READY GUEST v3-share2 putdata bigfile");
    assert!(!rig.h.big_share_active());
    assert!(!rig.h.queue.iter().any(|req| matches!(req, ServiceReq::ShareRange { .. })));
    rig.wait(|rig| rig.files().is_empty());
}

#[test]
fn clipboard_and_control_requests_go_out_between_ranges_and_other_share_requests_wait() {
    let mut rig = rig("interleave", 1 << 30);
    let bytes = data(2 * SHARE_RANGE_BYTES as usize + 3);
    let size = bytes.len() as u64;
    rig.list("big.bin", size, TIME);
    let first = rig.step();
    // Queued while the first range is on the wire.
    rig.h.queue.push_back(ServiceReq::ShareGet { name: "small.txt".into() });
    rig.h.queue.push_back(ServiceReq::Ctl("whoami".into()));
    rig.h.queue.push_back(ServiceReq::ClipPoll);
    for reply in range_reply(first.as_deref().unwrap(), &bytes, size, TIME) {
        rig.reply(&reply);
    }
    let next = rig.step().unwrap();
    assert!(next.starts_with("RUN "), "control waited for the copy: {next}");
    rig.reply("OUT 0 ");
    let next = rig.step().unwrap();
    assert!(next.starts_with("CLIPGET"), "the clipboard waited for the copy: {next}");
    rig.reply("CLIP ");
    let mut sent = Vec::new();
    loop {
        // As the service's every tick: the writer's answers, then the next request.
        rig.wait(|rig| writer_backlog(rig) <= SHARE_RANGE_BYTES);
        let Some(line) = rig.step() else { break };
        if line.starts_with("GETRANGE ") && line.split(' ').nth(2) != Some("0") {
            for reply in range_reply(&line, &bytes, size, TIME) {
                rig.reply(&reply);
            }
        } else {
            sent.push(line);
            break;
        }
    }
    rig.wait(|rig| !rig.h.big_share_active());
    let small = rig.step().unwrap_or_default();
    assert!(
        small.starts_with("GETRANGE ") && small.ends_with(" 0 8388608\n"),
        "the other file's fetch went before the copy ended: {sent:?} {small}"
    );
    assert_eq!(std::fs::read(rig.dir.join("big.bin")).unwrap(), bytes);
}

#[test]
fn a_large_guest_file_is_refused_when_the_mac_lacks_the_space_for_it() {
    // A listing larger than any disk this test runs on, within the cap.
    let mut rig = rig("space", u64::MAX / 1024);
    let size = 1u64 << 60;
    rig.list("huge.vhdx", size, TIME);
    assert!(rig.step().is_none(), "the fetch went out");
    assert!(!rig.h.big_share_active());
    assert!(rig.files().is_empty());
    let share = rig.h.share.as_ref().unwrap();
    assert!(share.low_space_seen.contains(&("huge.vhdx".to_string(), Some(TIME.to_string()))));
    // Asked again at the next listing (space may have been freed), reported once.
    rig.list("huge.vhdx", size, TIME);
    assert!(rig.step().is_none());
    assert_eq!(rig.h.share.as_ref().unwrap().low_space_seen.len(), 1);
}

#[test]
fn an_agent_without_ranges_keeps_the_whole_file_limit() {
    let mut rig = rig("old", 1 << 30);
    rig.h.share_big = false;
    let engine = &mut rig.h.share.as_mut().unwrap().engine;
    engine.set_whole_file_limit(Some(harness_protocol::MAX_AGENT_GET_BYTES as u64));
    rig.list("movie.mov", 100 << 20, TIME);
    assert!(rig.h.queue.is_empty(), "a file past 64 MiB was fetched whole");
    rig.list("clip.mov", 10 << 20, TIME);
    assert!(rig.step().is_some_and(|line| line.starts_with("GET ")));
    // The READY of an agent that moves ranges lifts it.
    rig.h.in_flight = None;
    rig.reply("READY GUEST v3-share2 putdata bigfile");
    assert!(rig.h.share_big);
    assert_eq!(rig.h.share.as_ref().unwrap().engine.max_bytes(), 1 << 40);
}

const GUEST_TEMP: &str = ".bridgevm-part-0123456789abcdef0123456789abcdef";

#[test]
fn the_host_scan_keeps_only_exact_temporaries_out_and_removes_one_left_by_a_dead_process() {
    let mut rig = rig("temps", 1 << 30);
    let mut child = std::process::Command::new("/usr/bin/true").spawn().unwrap();
    let dead = child.id();
    child.wait().unwrap();
    let stale = format!(".bridgevm-sync-{dead}-3");
    let live = format!(".bridgevm-sync-{}-4", std::process::id());
    // A user's own names that only look like ours are files like any other.
    let users = [".bridgevm-part-notes".to_string(), format!(".bridgevm-sync-{dead}-notes"), ".bridgevm-sync-notes".into()];
    for name in users.iter().chain([&stale, &live, &GUEST_TEMP.to_string(), &"kept.txt".to_string()]) {
        std::fs::write(rig.dir.join(name), b"x").unwrap();
    }
    let share = rig.h.share.as_mut().unwrap();
    assert!(scan_share_host_dir(share, rig.now).is_some());
    let mut scanned: Vec<_> = share.host_scan_scratch.iter().map(|file| file.name.clone()).collect();
    scanned.sort();
    let mut want: Vec<_> = users.to_vec();
    want.push("kept.txt".into());
    want.sort();
    assert_eq!(scanned, want, "only exact temporaries stay out of the scan");
    let mut left: Vec<_> = want.clone();
    left.extend([live, GUEST_TEMP.to_string()]);
    left.sort();
    assert_eq!(rig.files(), left, "only the dead process's own temporary is removed");
}

#[test]
fn a_guest_name_in_a_temporarys_exact_form_is_never_fetched_and_a_look_alike_is() {
    let mut rig = rig("guest-temps", 1 << 30);
    let names = [GUEST_TEMP, "sub/.bridgevm-sync-12-3", ".bridgevm-part-notes", "sub/.bridgevm-sync-notes"];
    let listing: String = names.iter().map(|name| format!("{name}|5|0|{TIME}\n")).collect();
    rig.h.in_flight = Some((ServiceReq::ShareLs, rig.now));
    rig.reply(&format!("LSOK {}", base64_encode(listing.as_bytes())));
    let mut fetched: Vec<_> = rig
        .h
        .queue
        .iter()
        .filter_map(|req| match req {
            ServiceReq::ShareGet { name } => Some(name.as_str()),
            _ => None,
        })
        .collect();
    fetched.sort();
    assert_eq!(fetched, [".bridgevm-part-notes", "sub/.bridgevm-sync-notes"]);
}
