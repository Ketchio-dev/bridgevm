//! Large Mac files to an agent with `bigfile`: read one chunk at a time, with other requests
//! between chunks, recorded once sent, abandoned when the Mac file changes or the agent
//! restarts, and held after a guest refusal until the Mac file changes.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

const ROOT: &str = "C:\\share";

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

fn rig(label: &str) -> Rig {
    let dir = std::env::temp_dir().join(format!("bvagent-bigput-{label}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let now = Instant::now();
    let mut h = harness();
    h.service = true;
    h.state = AgentConsoleState::Service;
    h.share_big = true;
    h.last_send = Some(now);
    let mut share = ShareState::new(ShareSync::new(1 << 30), dir.clone(), ROOT.into(), Duration::from_secs(3600));
    share.last_poll = Some(now);
    share.engine.end_first_listing_wait();
    h.share = Some(share);
    Rig { h, dir, now }
}

impl Rig {
    fn step(&mut self) -> Option<String> {
        let mut sent = Vec::new();
        self.h.advance_big_share(self.now);
        self.h
            .service_pump_with_sender(self.now, &mut |bytes| sent.push(String::from_utf8_lossy(bytes).into_owned()));
        sent.pop()
    }
    fn reply(&mut self, line: &str) {
        self.h.handle_service_reply(line, None, None, self.now);
    }
    /// The host step: a scan of the Mac folder, which may queue a PUT.
    fn scan(&mut self) -> bool {
        self.h.enqueue_one_share_host_change(self.now)
    }
    fn big_steps_queued(&self) -> usize {
        let queued = self.h.queue.iter().filter(|req| matches!(req, ServiceReq::ShareBigPut(_))).count();
        queued + usize::from(matches!(self.h.in_flight, Some((ServiceReq::ShareBigPut(_), _))))
    }
}

fn data(len: usize) -> Vec<u8> {
    (0..len).map(|i| (i * 13 + i / 5) as u8).collect()
}

/// Answer LPUTDATA steps until the LPUTEND goes out; the decoded chunks in order.
fn send_chunks(rig: &mut Rig, upto: Option<usize>) -> (Vec<u8>, Option<String>) {
    let mut got = Vec::new();
    let mut chunks = 0;
    let mut last: Option<usize> = None;
    loop {
        assert!(rig.big_steps_queued() <= 1, "more than one chunk held at once");
        let Some(line) = rig.step() else { return (got, None) };
        if !line.starts_with("LPUTDATA ") {
            return (got, Some(line));
        }
        let (head, payload) = line.split_once('\n').unwrap();
        let seq: usize = head.split(' ').nth(1).unwrap().parse().unwrap();
        assert!(last.is_none_or(|last| seq == last + 1), "chunk {seq} after {last:?}");
        last = Some(seq);
        let bytes = base64_decode(payload.trim_end()).unwrap();
        assert!(bytes.len() <= SHARE_PUT_DATA_CHUNK_BYTES);
        got.extend_from_slice(&bytes);
        rig.reply(&format!("OK LPUTDATA {seq}"));
        chunks += 1;
        if upto == Some(chunks) {
            return (got, None);
        }
    }
}

#[test]
fn a_large_mac_file_streams_in_chunks_with_other_requests_between_and_is_then_synced() {
    let mut rig = rig("whole");
    let bytes = data(SHARE_RANGE_BYTES as usize + 2 * SHARE_PUT_DATA_CHUNK_BYTES + 5);
    std::fs::write(rig.dir.join("video.mov"), &bytes).unwrap();
    assert!(rig.scan());
    let beg = rig.step().unwrap();
    assert!(beg.starts_with("LPUTBEG ") && beg.contains(&format!(" {} ", bytes.len())), "{beg}");
    rig.reply("OK LPUTBEG");
    let (first, _) = send_chunks(&mut rig, Some(1));
    // A control command and a clipboard poll queued mid-file go out before the next chunk.
    rig.h.queue.push_back(ServiceReq::Ctl("whoami".into()));
    rig.h.queue.push_back(ServiceReq::ClipPoll);
    assert!(rig.step().unwrap().starts_with("RUN "));
    rig.reply("OUT 0 ");
    assert!(rig.step().unwrap().starts_with("CLIPGET"));
    rig.reply("CLIP ");
    let (rest, end) = send_chunks(&mut rig, None);
    assert_eq!([first, rest].concat(), bytes);
    assert_eq!(end.as_deref(), Some("LPUTEND 11\n"));
    rig.reply(&format!("PUTOK {} {}", base64_encode(b"C:\\share\\video.mov"), bytes.len()));
    assert!(!rig.h.big_share_active());
    // Recorded as synced: the next scan sends nothing again.
    rig.h.share.as_mut().unwrap().host_actions.clear();
    assert!(!rig.scan(), "the file was queued again: {:?}", rig.h.queue.iter().map(req_kind).collect::<Vec<_>>());
    assert!(!rig.h.big_share_active());
}

#[test]
fn a_mac_file_changed_mid_send_is_aborted_and_sent_again() {
    let mut rig = rig("changed");
    let bytes = data(SHARE_RANGE_BYTES as usize + 1);
    std::fs::write(rig.dir.join("doc.bin"), &bytes).unwrap();
    assert!(rig.scan());
    rig.step();
    rig.reply("OK LPUTBEG");
    send_chunks(&mut rig, Some(2));
    // The Mac app saves again while the file is on its way.
    let mut file = std::fs::OpenOptions::new().append(true).open(rig.dir.join("doc.bin")).unwrap();
    std::io::Write::write_all(&mut file, b"more").unwrap();
    assert_eq!(rig.step().as_deref(), Some("LPUTABORT\n"), "the half-sent file was not abandoned");
    assert!(!rig.h.big_share_active());
    rig.reply("OK LPUTABORT");
    rig.h.share.as_mut().unwrap().host_actions.clear();
    assert!(rig.scan(), "the changed file was not sent again");
    assert!(rig.step().unwrap().contains(&format!(" {} ", bytes.len() + 4)));
}

#[test]
fn an_agent_restart_mid_send_abandons_the_file_and_the_next_scan_sends_it_again() {
    let mut rig = rig("restart");
    std::fs::write(rig.dir.join("big.iso"), data(SHARE_RANGE_BYTES as usize + 1)).unwrap();
    assert!(rig.scan());
    rig.step();
    rig.reply("OK LPUTBEG");
    send_chunks(&mut rig, Some(1));
    rig.step();
    rig.reply("READY GUEST v3-share2 putdata bigfile");
    assert!(!rig.h.big_share_active());
    assert_eq!(rig.big_steps_queued(), 0, "a step went to the new agent");
    rig.h.share.as_mut().unwrap().host_actions.clear();
    assert!(rig.scan());
}

#[test]
fn a_guest_refusal_holds_the_file_until_the_mac_copy_changes() {
    let mut rig = rig("refused");
    let path = rig.dir.join("big.bin");
    std::fs::write(&path, data(SHARE_RANGE_BYTES as usize + 1)).unwrap();
    assert!(rig.scan());
    rig.step();
    rig.reply("ERR LPUTBEG disk-full");
    assert!(!rig.h.big_share_active() && rig.h.in_flight.is_none());
    rig.h.share.as_mut().unwrap().host_actions.clear();
    assert!(!rig.scan(), "a refused file was sent again at once");
    let file = std::fs::File::options().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(5)).unwrap();
    rig.h.share.as_mut().unwrap().host_actions.clear();
    assert!(rig.scan(), "the changed file was not sent");
}

#[test]
fn an_agent_without_bigfile_gets_the_whole_file_put_as_before() {
    let mut rig = rig("old");
    rig.h.share_big = false;
    rig.h.put_data = true;
    std::fs::write(rig.dir.join("big.bin"), data(SHARE_RANGE_BYTES as usize + 1)).unwrap();
    assert!(rig.scan());
    assert!(rig.step().unwrap().starts_with("PUTBEG "));
}

#[test]
fn a_new_session_adopts_a_large_guest_copy_of_the_same_size_and_time_without_sending_it() {
    let mut rig = rig("adopt");
    let path = rig.dir.join("kept.mkv");
    let size = SHARE_RANGE_BYTES + 7;
    std::fs::File::create(&path).unwrap().set_len(size).unwrap();
    let stamp = std::time::UNIX_EPOCH + Duration::from_millis(1_794_096_312_123);
    std::fs::File::options().write(true).open(&path).unwrap().set_modified(stamp).unwrap();
    let entry = LsEntry {
        name: "kept.mkv".into(),
        size,
        is_dir: false,
        mtime: "2026-11-08T00:05:12.1230000Z".into(),
    };
    // A new session: the scan holds the unrecorded Mac file until the first listing.
    rig.h.share.as_mut().unwrap().engine = ShareSync::new(1 << 30).into();
    assert!(!rig.scan());
    let engine = &mut rig.h.share.as_mut().unwrap().engine;
    assert!(engine.on_guest_listing_normalized([entry]).is_empty());
    rig.h.share.as_mut().unwrap().host_actions.clear();
    assert!(!rig.scan(), "the guest's copy was sent again");
    assert!(!rig.h.big_share_active());
}
