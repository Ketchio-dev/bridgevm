//! A file-sync backlog cannot reject or delay user traffic; its own order remains intact.

use super::*;
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use crate::agent_console::agent_console_tests::tests::harness;
use bridgevm_hvf::dtb::VirtFdtConfig;
use std::io::Write;

struct NoGuestMemory;
impl GuestMemoryMut for NoGuestMemory {
    fn write_bytes(&mut self, _: u64, _: &[u8]) -> bool {
        false
    }
    fn read_bytes(&self, _: u64, _: usize) -> Option<Vec<u8>> {
        None
    }
}

#[test]
fn share_backlog_keeps_clipboard_then_control_order_without_preempting_the_wire() {
    let mut h = harness();
    for i in 0..=MAX_SERVICE_QUEUE_DEPTH {
        h.queue.push_back(ServiceReq::ShareGet {
            name: i.to_string(),
        });
    }
    h.queue.push_back(ServiceReq::ClipPush("copied".into()));
    let path =
        std::env::temp_dir().join(format!("bvagent-share-priority-{}.ctl", std::process::id()));
    let mut ctl = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&path)
        .unwrap();
    ctl.write_all(b"PING\n").unwrap();
    h.ctl_path = Some(path.to_string_lossy().into_owned());
    h.ingest_ctl();
    assert!(
        h.queue
            .iter()
            .any(|r| matches!(r, ServiceReq::Ctl(c) if c == "PING")),
        "share backlog refused user input"
    );
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    let mut mem = NoGuestMemory;
    let now = Instant::now();
    h.service_pump(&mut platform, &mut mem, now);
    assert_eq!(
        h.service_line_scratch,
        format!("CLIPSET {}\n", base64_encode(b"copied"))
    );
    h.service_pump(&mut platform, &mut mem, now + Duration::from_millis(1));
    assert!(
        matches!(h.in_flight.as_ref(), Some((ServiceReq::ClipPush(_), _))),
        "control preempted the clipboard transaction"
    );
    h.handle_service_reply("OK CLIPSET", None, None, now + Duration::from_millis(2));
    h.service_pump(&mut platform, &mut mem, now + Duration::from_millis(3));
    assert_eq!(h.service_line_scratch, "PING\n");
    assert!(
        matches!(pop_next_request(&mut h.queue), Some(ServiceReq::ShareGet { name }) if name == "0")
    );
    assert!(
        matches!(pop_next_request(&mut h.queue), Some(ServiceReq::ShareGet { name }) if name == "1")
    );
    drop(ctl);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn foreground_queue_ceiling_is_preserved_alongside_background_share_work() {
    let mut queue = VecDeque::new();
    queue.push_back(ServiceReq::ShareLs);
    for i in 0..MAX_SERVICE_QUEUE_DEPTH - 1 {
        queue.push_back(ServiceReq::Ctl(i.to_string()));
    }
    assert!(has_foreground_capacity(&queue));
    queue.push_back(ServiceReq::Ping);
    assert!(!has_foreground_capacity(&queue));
    assert!(matches!(pop_next_request(&mut queue), Some(ServiceReq::Ctl(c)) if c == "0"));
    assert!(has_foreground_capacity(&queue));
}
