//! An expired host press may still occupy the relay. Cleanup cannot overtake it.
#![cfg(test)]

use super::agent_console_tests::tests::harness;
use super::*;
use bridgevm_hvf::dtb::VirtFdtConfig;

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
fn overdue_pointer_press_keeps_its_slot_ahead_of_one_shot_cleanup() {
    let mut h = harness();
    h.state = AgentConsoleState::Service;
    h.service = true;
    let id = "12345678-1234-1234-1234-123456789abc";
    let release_id = "12345678-1234-1234-1234-123456789abd";
    let press = format!("POINTERINPUT {id} {}", base64_encode(b"press:1x2"));
    let release = format!(
        "POINTERINPUT {release_id} {}",
        base64_encode(b"releaseall:1x2")
    );
    h.queue.push_back(ServiceReq::Ctl(press.clone()));
    h.queue.push_back(ServiceReq::Ctl(release.clone()));
    let mut platform = VirtPlatform::new(VirtFdtConfig::default());
    let mut mem = NoGuestMemory;
    let start = Instant::now();
    h.service_pump(&mut platform, &mut mem, start);
    assert_eq!(h.service_line_scratch, format!("{press}\n"));
    for beat in 1..=6 {
        h.service_pump(
            &mut platform,
            &mut mem,
            start + SERVICE_OVERDUE_INTERVAL * beat,
        );
        assert!(matches!(h.in_flight.as_ref(), Some((ServiceReq::Ctl(c), _)) if c == &press));
        assert_eq!(
            h.queue.len(),
            1,
            "overdue input was retransmitted or cleanup overtook it"
        );
        assert!(matches!(h.queue.front(), Some(ServiceReq::Ctl(c)) if c == &release));
        assert_eq!(h.service_line_scratch, format!("{press}\n"));
    }
    let now = start + SERVICE_OVERDUE_INTERVAL * 7;
    let reply = format!(
        "OUT 0 {}",
        base64_encode(format!("BVINPUT_INSERTED {id} 1").as_bytes())
    );
    h.handle_service_reply(&reply, None, None, now);
    assert!(h.in_flight.is_none());
    h.service_pump(&mut platform, &mut mem, now);
    assert_eq!(h.service_line_scratch, format!("{release}\n"));
    assert!(matches!(h.in_flight.as_ref(), Some((ServiceReq::Ctl(c), _)) if c == &release));
    assert!(h.queue.is_empty());
    // The original request was completed, never placed behind its cleanup.
    h.service_pump(&mut platform, &mut mem, now + SERVICE_OVERDUE_INTERVAL);
    assert!(h.queue.is_empty());
    assert_eq!(h.service_line_scratch, format!("{release}\n"));
}
