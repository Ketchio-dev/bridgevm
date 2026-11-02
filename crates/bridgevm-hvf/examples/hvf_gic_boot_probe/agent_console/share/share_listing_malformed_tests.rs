//! Malformed snapshots must not delete host files or queue a valid-looking prefix.

use super::*;

struct HostDirectory(PathBuf);
impl HostDirectory {
    fn new(label: &str) -> Self {
        for nonce in 0..128 {
            let path = std::env::temp_dir().join(format!(
                "bvagent-malformed-{label}-{}-{nonce}",
                std::process::id()
            ));
            match std::fs::create_dir(&path) {
                Ok(()) => return Self(path),
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                Err(error) => panic!("create owned directory: {error}"),
            }
        }
        panic!("no fresh owned directory available");
    }
}
impl Drop for HostDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn owned_share(label: &str) -> (AgentConsoleHarness, HostDirectory) {
    let directory = HostDirectory::new(label);
    let mut h = synced_share();
    h.share.as_mut().unwrap().host_dir = directory.0.clone();
    std::fs::write(directory.0.join("keep.txt"), [7]).unwrap();
    (h, directory)
}

fn recover_then_delete_only_on_explicit_empty(
    h: &mut AgentConsoleHarness,
    directory: &HostDirectory,
) {
    reply(h, b"keep.txt|1|0|m1\n");
    assert!(
        h.queue.is_empty(),
        "a refused snapshot changed the cached file version"
    );
    assert_eq!(std::fs::read(directory.0.join("keep.txt")).unwrap(), [7]);
    reply(h, b"");
    let request = h
        .queue
        .pop_front()
        .expect("genuine empty snapshot must plan deletion");
    assert!(
        matches!(&request, ServiceReq::ShareDel { name, direction: ShareDelDirection::GuestToHost } if name == "keep.txt")
    );
    assert!(h.service_req_line(&request, Instant::now()).is_none());
    assert!(!directory.0.join("keep.txt").exists());
}

#[test]
fn malformed_listing_first_row_never_deletes_a_synced_host_file() {
    for malformed in [
        "malformed\n",
        "keep.txt|not-a-size|0|m1\n",
        "keep.txt|18446744073709551616|0|m1\n",
        "keep.txt|1|invalid|m2\n",
    ] {
        let (mut h, directory) = owned_share("first");
        reply(&mut h, malformed.as_bytes());
        let mut sent = Vec::new();
        while let Some(request) = h.queue.pop_front() {
            if let Some(wire) = h.service_req_line(&request, Instant::now()) {
                sent.push(wire.to_owned());
            }
        }
        assert_eq!(
            std::fs::read(directory.0.join("keep.txt")).ok(),
            Some(vec![7]),
            "malformed listing deleted the owned host file"
        );
        assert!(sent.is_empty(), "malformed listing planned file transfer");
        recover_then_delete_only_on_explicit_empty(&mut h, &directory);
    }
}

#[test]
fn malformed_listing_later_row_never_plans_partial_work() {
    for malformed in [
        "fresh.txt|1|0|m2\nkeep.txt|broken|0|m1\n",
        "keep.txt|1|0|m1\nfresh.txt|1|2|m2\n",
    ] {
        let (mut h, directory) = owned_share("later");
        reply(&mut h, malformed.as_bytes());
        assert!(
            h.queue.is_empty(),
            "a malformed trailing row queued a prefix GET or host deletion"
        );
        recover_then_delete_only_on_explicit_empty(&mut h, &directory);
    }
}
