//! Finder drops: wire order, chunk streaming, folder renames, old agents, stalls, interleaving.

use super::*;
use crate::agent_console::agent_console_tests::tests::harness;

fn b64(bytes: &[u8]) -> String {
    let mut out = String::new();
    base64_encode_into(bytes, &mut out);
    out
}

fn staging(label: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("bvdrop-{}-{label}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

/// A resident harness whose every `reply` runs one `advance` at one fixed instant: whatever it
/// sends left because of that reply, not because time passed or a periodic tick came round.
struct Wire {
    h: AgentConsoleHarness,
    now: Instant,
}

impl Wire {
    fn new() -> Self {
        let now = Instant::now();
        let mut h = harness();
        h.state = AgentConsoleState::Service;
        h.service = true;
        h.last_send = Some(now);
        h.last_alive = Some(now);
        Self { h, now }
    }

    fn submit(&mut self, id: &str, root: &Path) -> Vec<String> {
        self.h
            .drops
            .submit(&format!("{id} {}", b64(root.to_str().unwrap().as_bytes())));
        self.tick()
    }

    fn tick(&mut self) -> Vec<String> {
        let mut sent = Vec::new();
        self.h.advance(self.now, &mut |bytes| {
            sent.push(String::from_utf8_lossy(bytes).trim_end().to_string());
        });
        sent
    }

    fn reply(&mut self, line: &str) -> Vec<String> {
        self.h.inbound_scratch.extend_from_slice(line.as_bytes());
        self.h.inbound_scratch.push(b'\n');
        self.tick()
    }
}

/// The header names the payload's base64 length; the payload follows as its own line.
fn chunk_payload(sent: &str, seq: u64) -> Vec<u8> {
    let (header, payload) = sent
        .split_once('\n')
        .unwrap_or_else(|| panic!("expected a header and a payload, got {:.60}", sent));
    assert_eq!(header, format!("DROPCHUNK {seq} {}", payload.len()));
    base64_decode(payload).unwrap()
}

#[test]
fn next_chunk_leaves_in_the_same_tick_as_the_guest_reply() {
    let root = staging("stream");
    let size = 2 * DROP_CHUNK_BYTES + DROP_CHUNK_BYTES / 2;
    let content: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
    std::fs::write(root.join("big.bin"), &content).unwrap();
    let ctl = root.with_extension("ctl");
    std::fs::write(
        &ctl,
        format!("BVDROP job-1 {}\n", b64(root.to_str().unwrap().as_bytes())),
    )
    .unwrap();

    let mut wire = Wire::new();
    wire.h.ctl_path = Some(ctl.to_string_lossy().into_owned());
    // The control line becomes a job, never a guest command.
    assert_eq!(wire.tick(), ["DROPCAPS"]);
    assert_eq!(
        wire.reply("BVDROP_CAPS 1"),
        [format!("DROPBEG {} {size}", b64(b"big.bin"))]
    );
    let mut received = Vec::new();
    let sent = wire.reply(&format!("OK DROPBEG {}", b64(b"big.bin")));
    received.extend(chunk_payload(&sent[0], 0));
    for seq in 0..2 {
        // Each acknowledgement sends the next chunk at once, at the same instant.
        let sent = wire.reply(&format!("OK DROPCHUNK {seq}"));
        assert_eq!(sent.len(), 1);
        received.extend(chunk_payload(&sent[0], seq + 1));
    }
    assert_eq!(wire.reply("OK DROPCHUNK 2"), ["DROPEND 3"]);
    assert_eq!(received, content);
    assert!(wire.reply(&format!("OK DROPEND {size}")).is_empty());
    assert!(wire.h.drops.active.is_none());
    assert!(wire.h.queue.is_empty() && wire.h.in_flight.is_none());
    let _ = std::fs::remove_dir_all(&root);
    let _ = std::fs::remove_file(&ctl);
}

#[test]
fn folders_go_first_symlinks_are_skipped_and_a_renamed_folder_keeps_its_contents() {
    let root = staging("tree");
    std::fs::create_dir(root.join("A")).unwrap();
    std::fs::write(root.join("A/b.txt"), b"hi").unwrap();
    std::fs::write(root.join("c.txt"), b"").unwrap();
    std::os::unix::fs::symlink("/etc/hosts", root.join("link")).unwrap();
    std::os::unix::fs::symlink("../c.txt", root.join("A/inner")).unwrap();

    let mut wire = Wire::new();
    assert_eq!(wire.submit("job-2", &root), ["DROPCAPS"]);
    assert_eq!(wire.reply("BVDROP_CAPS 1"), [format!("DROPDIR {}", b64(b"A"))]);
    // The Desktop already has an "A": the guest made "A (2)", and b.txt follows it there.
    assert_eq!(
        wire.reply(&format!("OK DROPDIR {}", b64(b"A (2)"))),
        [format!("DROPBEG {} 2", b64(b"A (2)\\b.txt"))]
    );
    assert_eq!(
        wire.reply(&format!("OK DROPBEG {}", b64(b"b.txt"))),
        [format!("DROPCHUNK 0 4\n{}", b64(b"hi"))]
    );
    assert_eq!(wire.reply("OK DROPCHUNK 0"), ["DROPEND 1"]);
    assert_eq!(
        wire.reply("OK DROPEND 2"),
        [format!("DROPBEG {} 0", b64(b"c.txt"))]
    );
    assert_eq!(
        wire.reply(&format!("OK DROPBEG {}", b64(b"c.txt"))),
        ["DROPEND 0"]
    );
    assert!(wire.reply("OK DROPEND 0").is_empty());
    assert!(wire.h.drops.active.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn an_agent_without_drop_verbs_ends_the_job_before_any_write() {
    let root = staging("old-agent");
    std::fs::write(root.join("a.txt"), b"x").unwrap();
    let mut wire = Wire::new();
    assert_eq!(wire.submit("job-3", &root), ["DROPCAPS"]);
    let unknown = format!("OUT 255 {}", b64(b"unknown token: DROPCAPS"));
    assert!(wire.reply(&unknown).is_empty());
    assert!(wire.h.drops.active.is_none());
    assert!(wire.h.queue.is_empty() && wire.h.in_flight.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_control_command_queued_mid_file_goes_out_before_the_next_chunk() {
    let root = staging("interleave");
    std::fs::write(root.join("big.bin"), vec![7u8; DROP_CHUNK_BYTES + 1]).unwrap();
    let mut wire = Wire::new();
    wire.submit("job-4", &root);
    wire.reply("BVDROP_CAPS 1");
    let sent = wire.reply(&format!("OK DROPBEG {}", b64(b"big.bin")));
    assert!(sent[0].starts_with("DROPCHUNK 0 "));
    // The user stops the VM while chunk 0 is on the wire.
    wire.h
        .queue
        .push_back(ServiceReq::Ctl("shutdown.exe /p /f".into()));
    assert_eq!(
        wire.reply("OK DROPCHUNK 0"),
        [format!("RUN {}", b64(b"shutdown.exe /p /f"))]
    );
    let sent = wire.reply(&format!("OUT 0 {}", b64(b"")));
    assert_eq!(chunk_payload(&sent[0], 1), [7u8]);
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stalled_drop_step_fails_the_job_instead_of_resending_it() {
    let root = staging("stall");
    std::fs::write(root.join("big.bin"), vec![1u8; 10]).unwrap();
    let mut wire = Wire::new();
    wire.submit("job-5", &root);
    wire.reply("BVDROP_CAPS 1");
    wire.reply(&format!("OK DROPBEG {}", b64(b"big.bin")));
    let mut sent = Vec::new();
    for beat in 1..=3u32 {
        let at = wire.now + SERVICE_OVERDUE_INTERVAL * beat;
        wire.h.service_pump_with_sender(at, &mut |bytes| {
            sent.push(String::from_utf8_lossy(bytes).trim_end().to_string());
        });
    }
    // The half-written guest file is deleted; chunk 0 is not sent twice.
    assert_eq!(sent, ["DROPABORT"]);
    assert!(wire.h.drops.active.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_stalled_dropbeg_aborts_the_file_even_though_its_acknowledgement_never_came() {
    let root = staging("lost-beg-ack");
    std::fs::write(root.join("a.txt"), b"x").unwrap();
    let mut wire = Wire::new();
    wire.submit("job-8", &root);
    assert_eq!(
        wire.reply("BVDROP_CAPS 1"),
        [format!("DROPBEG {} 1", b64(b"a.txt"))]
    );
    // The guest may have created and opened a.txt; its OK DROPBEG is lost.
    let mut sent = Vec::new();
    for beat in 1..=3u32 {
        let at = wire.now + SERVICE_OVERDUE_INTERVAL * beat;
        wire.h.service_pump_with_sender(at, &mut |bytes| {
            sent.push(String::from_utf8_lossy(bytes).trim_end().to_string());
        });
    }
    assert_eq!(sent, ["DROPABORT"]);
    assert!(wire.h.drops.active.is_none());
    // A late acknowledgement is not the abort's reply.
    wire.now += SERVICE_OVERDUE_INTERVAL * 3;
    assert!(wire.reply(&format!("OK DROPBEG {}", b64(b"a.txt"))).is_empty());
    assert!(wire.h.any_pending(is_drop_req));
    assert!(wire.reply("OK DROPABORT").is_empty());
    assert!(!wire.h.any_pending(is_drop_req));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_drop_ended_by_an_agent_restart_sends_none_of_its_queued_steps() {
    let root = staging("restart");
    std::fs::write(root.join("a.txt"), b"x").unwrap();
    let mut wire = Wire::new();
    wire.submit("job-9", &root);
    // A command goes out first, so the DROPBEG waits behind it.
    wire.h.queue.push_back(ServiceReq::Ctl("whoami".into()));
    assert_eq!(
        wire.reply("BVDROP_CAPS 1"),
        [format!("RUN {}", b64(b"whoami"))]
    );
    // The agent restarts before answering: the new one must not get the dead job's DROPBEG.
    let sent = wire.reply("READY host");
    assert!(sent.iter().all(|line| !line.starts_with("DROP")), "{sent:?}");
    assert!(!wire.h.any_pending(is_drop_req));
    assert!(wire.h.drops.active.is_none());
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn a_second_drop_waits_for_the_first() {
    let first = staging("queue-1");
    let second = staging("queue-2");
    std::fs::write(first.join("one.txt"), b"1").unwrap();
    std::fs::write(second.join("two.txt"), b"2").unwrap();
    let mut wire = Wire::new();
    assert_eq!(wire.submit("job-6", &first), ["DROPCAPS"]);
    assert!(wire.submit("job-7", &second).is_empty());
    wire.reply("BVDROP_CAPS 1");
    wire.reply(&format!("OK DROPBEG {}", b64(b"one.txt")));
    wire.reply("OK DROPCHUNK 0");
    // The first job's last reply starts the second one in the same tick.
    assert_eq!(wire.reply("OK DROPEND 1"), ["DROPCAPS"]);
    assert_eq!(
        wire.reply("BVDROP_CAPS 1"),
        [format!("DROPBEG {} 1", b64(b"two.txt"))]
    );
    let _ = std::fs::remove_dir_all(&first);
    let _ = std::fs::remove_dir_all(&second);
}

#[test]
fn shared_folder_sync_takes_turns_with_a_drop_but_user_traffic_goes_first() {
    use crate::agent_console::service_queue::pop_next_request;
    let mut queue = VecDeque::from([
        ServiceReq::ShareLs,
        ServiceReq::Drop(DropReq::End { chunks: 1 }),
        ServiceReq::Ping,
    ]);
    assert!(matches!(pop_next_request(&mut queue, |_| false), Some(ServiceReq::Ping)));
    assert!(matches!(pop_next_request(&mut queue, |_| false), Some(ServiceReq::ShareLs)));
    assert!(matches!(pop_next_request(&mut queue, |_| false), Some(ServiceReq::Drop(_))));
}
