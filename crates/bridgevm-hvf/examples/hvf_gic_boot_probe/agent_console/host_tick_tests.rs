//! The host-side advance path must send reply-driven work and complete host I/O first.

use crate::agent_console::agent_console_tests::tests::harness;
use crate::agent_console::*;
use std::path::PathBuf;

struct HostDirectory(PathBuf);
impl HostDirectory {
    fn new(label: &str) -> Self {
        for nonce in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "bvagent-host-advance-{label}-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned host directory: {error}"),
            }
        }
        panic!("no fresh host directory available");
    }
}
impl Drop for HostDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn resident(directory: &HostDirectory, now: Instant) -> AgentConsoleHarness {
    let mut h = harness();
    h.start = now;
    h.state = AgentConsoleState::Service;
    h.service = true;
    h.last_send = Some(now);
    h.last_alive = Some(now);
    h.share = Some(ShareState::new(
        ShareSync::new(1024),
        directory.0.clone(),
        "C:\\share".into(),
        Duration::from_secs(1),
    ));
    h.share.as_mut().unwrap().last_poll = Some(now);
    h
}

fn advance(h: &mut AgentConsoleHarness, inbound: &str, now: Instant) -> Vec<String> {
    h.inbound_scratch.extend_from_slice(inbound.as_bytes());
    let mut sent = Vec::new();
    h.advance(now, &mut |bytes| {
        sent.push(String::from_utf8(bytes.to_vec()).unwrap())
    });
    sent
}

#[test]
fn advance_sends_every_chunk_and_end_before_completing_the_put() {
    let directory = HostDirectory::new("put");
    let now = Instant::now();
    let mut h = resident(&directory, now);
    let source: Vec<u8> = (0..SHARE_PUT_CHUNK_BYTES + 17)
        .map(|i| (i % 251) as u8)
        .collect();
    let path = base64_encode(b"C:\\share\\big.bin");
    h.queue
        .push_back(share_put_req("big.bin".into(), source.clone(), 1));
    assert_eq!(
        advance(&mut h, "", now),
        [format!("PUTBEG {path} {} 2\n", source.len())]
    );
    assert_eq!(
        advance(&mut h, "OK PUTBEG\n", now),
        [format!(
            "PUTCHUNK 0 {}\n",
            base64_encode(&source[..SHARE_PUT_CHUNK_BYTES])
        )]
    );
    assert_eq!(
        advance(&mut h, "OK PUTCHUNK 0\n", now),
        [format!(
            "PUTCHUNK 1 {}\n",
            base64_encode(&source[SHARE_PUT_CHUNK_BYTES..])
        )]
    );
    assert_eq!(advance(&mut h, "OK PUTCHUNK 1\n", now), ["PUTEND 2\n"]);
    assert!(matches!(
        h.in_flight.as_ref(),
        Some((
            ServiceReq::SharePut {
                phase: SharePutPhase::End,
                ..
            },
            _
        ))
    ));
    assert!(advance(&mut h, &format!("PUTOK {path} {}\n", source.len()), now).is_empty());
    assert!(h.in_flight.is_none());
    assert!(h.queue.is_empty());
}

#[test]
fn advance_writes_the_completed_get_before_sending_queued_control() {
    let directory = HostDirectory::new("get");
    let now = Instant::now();
    let mut h = resident(&directory, now);
    let path = base64_encode(b"C:\\share\\received.txt");
    let content = b"guest result";
    h.queue.push_back(ServiceReq::ShareGet {
        name: "received.txt".into(),
    });
    assert_eq!(advance(&mut h, "", now), [format!("GET {path}\n")]);
    h.queue.push_back(ServiceReq::Ctl("PING".into()));
    h.inbound_scratch.extend_from_slice(
        format!(
            "GETBEG {path} {} 1\nGETCHUNK 0 {}\nGETEND 1\n",
            content.len(),
            base64_encode(content)
        )
        .as_bytes(),
    );
    let mut sent = Vec::new();
    h.advance(now, &mut |bytes| {
        assert_eq!(
            std::fs::read(directory.0.join("received.txt")).unwrap(),
            content,
            "wire send happened before the completed host write"
        );
        sent.push(bytes.to_vec());
    });
    assert_eq!(sent, [b"PING\n".to_vec()]);
    assert!(h.get_accum.is_none());
    assert!(
        matches!(h.in_flight.as_ref(), Some((ServiceReq::Ctl(command), _)) if command == "PING")
    );
}
