use super::super::{InputControlFile, LiveInputController, POLL_INTERVAL};
use crate::xhci_hid_input::test_support::{
    configure_dci3_and_dci5_interrupt_in_over_bar0, new_platform_and_ram, program_xhci_bar0,
    write_dci3_normal_trb, DCI3_KEY_BUFFER, DCI3_RING, TRB_SIZE,
};
use std::collections::VecDeque;
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn live_keyboard_uses_command_now_instead_of_stale_cached_time() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..2 {
        write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE * index,
            DCI3_KEY_BUFFER + 0x20 * index);
    }
    platform.set_xhci_report_interval(Duration::from_secs(1));
    let base = Instant::now(); platform.set_host_now(base);
    let path = std::env::temp_dir().join(format!("b4-live-key-clock-{}.ctl", std::process::id()));
    fs::write(&path, b"KEY enter\n").unwrap();
    let command_now = base + Duration::from_millis(900);
    let mut input = LiveInputController { source: Some(InputControlFile::from_path(path.clone())), offset: 0,
        lines: Default::default(), pending: VecDeque::new(), accepted_pointer_moves: 0,
        next_poll: command_now - POLL_INTERVAL };
    input.tick(&mut platform, &mut mem, command_now, false);
    assert_eq!(platform.xhci_setup_input_report_stats().emitted_key_reports, 1);
    platform.set_host_now(base + Duration::from_secs(1));
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(platform.xhci_setup_input_report_stats().emitted_release_reports, 0);
    platform.set_host_now(command_now + Duration::from_secs(1));
    platform.drain_xhci_setup_input_reports(&mut mem);
    assert_eq!(platform.xhci_setup_input_report_stats().emitted_release_reports, 1);
    fs::remove_file(path).unwrap();
}
