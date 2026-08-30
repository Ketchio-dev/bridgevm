use super::super::{InputControlFile, LiveInputController, POLL_INTERVAL};
use crate::xhci_hid_input::test_support::{
    configure_dci3_and_dci5_interrupt_in_over_bar0, new_platform_and_ram, program_xhci_bar0,
    write_dci5_normal_trb, DCI5_POINTER_BUFFER, DCI5_RING, TRB_SIZE,
};
use std::collections::VecDeque;
use std::fs;
use std::time::{Duration, Instant};

#[test]
fn live_pointer_uses_command_now_instead_of_stale_cached_time() {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_and_dci5_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..2 {
        write_dci5_normal_trb(&mut mem, DCI5_RING + TRB_SIZE * index,
            DCI5_POINTER_BUFFER + 0x20 * index);
    }
    platform.set_xhci_report_interval(Duration::from_secs(1));
    let base = Instant::now(); platform.set_host_now(base);
    let path = std::env::temp_dir().join(format!("b4-live-clock-{}.ctl", std::process::id()));
    fs::write(&path, b"POINTER click:16384x16384\n").unwrap();
    let command_now = base + Duration::from_millis(900);
    let mut input = LiveInputController { source: Some(InputControlFile::from_path(path.clone())), offset: 0,
        partial: String::new(), pending: VecDeque::new(), accepted_pointer_moves: 0,
        next_poll: command_now - POLL_INTERVAL };
    input.tick(&mut platform, &mut mem, command_now, false);
    assert_eq!(platform.xhci_pointer_input_report_stats().emitted_button_reports, 1); assert_eq!(platform.xhci_pointer_report_deadline(), Some(command_now + Duration::from_secs(1)));
    platform.set_host_now(base + Duration::from_secs(1));
    platform.drain_xhci_pointer_input_reports(&mut mem);
    assert_eq!(platform.xhci_pointer_input_report_stats().emitted_release_reports, 0);
    platform.set_host_now(command_now + Duration::from_secs(1));
    platform.drain_xhci_pointer_input_reports(&mut mem);
    assert_eq!(platform.xhci_pointer_input_report_stats().emitted_release_reports, 1);
    fs::remove_file(path).unwrap();
}
