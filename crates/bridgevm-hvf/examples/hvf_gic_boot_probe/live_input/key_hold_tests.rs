//! `KEY down:<key>` keeps the key in the boot keyboard's reports until `KEY up:<key>`: one report
//! per change, none in between, so Windows sees one continuous press and repeats it itself
//! (kbdhid's typematic). `KEY <key>` stays a tap, and keeps a held key down around it.

use super::*;
use crate::xhci_hid_input::test_support::{
    configure_dci3_interrupt_in_over_bar0, new_platform_and_ram, program_xhci_bar0, read_bytes,
    ring_dci3_doorbell, write_dci3_normal_trb, DCI3_KEY_BUFFER, DCI3_RING, TRB_SIZE,
};
use bridgevm_hvf::fwcfg::GuestMemoryMut;
use bridgevm_hvf::platform_virt::{FlatGuestRam, VirtPlatform};

const ARMED: [u8; 8] = [0xaa; 8];

fn controller() -> LiveInputController {
    LiveInputController {
        source: None,
        offset: 0,
        lines: Default::default(),
        power_scanned_to: 0,
        pending: VecDeque::new(),
        accepted_pointer_moves: 0,
        next_poll: Instant::now(),
    }
}

/// A keyboard whose guest has armed `count` interrupt-IN buffers, each filled with `ARMED`.
fn keyboard(count: u64) -> (VirtPlatform, FlatGuestRam) {
    let (mut platform, mut mem) = new_platform_and_ram();
    program_xhci_bar0(&mut platform, &mut mem);
    configure_dci3_interrupt_in_over_bar0(&mut platform, &mut mem);
    for index in 0..count {
        write_dci3_normal_trb(&mut mem, DCI3_RING + TRB_SIZE * index, buffer(index));
        assert!(mem.write_bytes(buffer(index), &ARMED));
    }
    (platform, mem)
}

fn buffer(index: u64) -> u64 {
    DCI3_KEY_BUFFER + 0x20 * index
}

fn apply(input: &mut LiveInputController, platform: &mut VirtPlatform, mem: &mut FlatGuestRam, line: &str) {
    input.push_line(line);
    input.apply_pending(platform, mem, Instant::now());
    assert!(input.pending.is_empty(), "{line} was taken");
}

#[test]
fn a_held_key_stays_in_the_report_until_its_up_line() {
    let (mut platform, mut mem) = keyboard(6);
    let mut input = controller();
    apply(&mut input, &mut platform, &mut mem, "KEY down:left");
    assert_eq!(read_bytes(&mem, buffer(0), 8), [0, 0, 0x50, 0, 0, 0, 0, 0]);
    // The guest keeps polling: no report while the key is held (its next buffer stays armed).
    for _ in 0..3 {
        ring_dci3_doorbell(&mut platform, &mut mem);
    }
    assert_eq!(read_bytes(&mem, buffer(1), 8), ARMED, "Left is still down");
    let stats = platform.xhci_setup_input_report_stats();
    assert_eq!((stats.emitted_key_reports, stats.emitted_release_reports), (1, 0));
    // Text typed meanwhile is a tap that keeps Left down.
    apply(&mut input, &mut platform, &mut mem, "KEY text-hex:61");
    assert_eq!(read_bytes(&mem, buffer(1), 8), [0, 0, 0x50, 0x04, 0, 0, 0, 0]);
    assert_eq!(read_bytes(&mem, buffer(2), 8), [0, 0, 0x50, 0, 0, 0, 0, 0]);
    // A second press of the held key changes nothing; its release is one empty report.
    apply(&mut input, &mut platform, &mut mem, "KEY down:left");
    assert_eq!(read_bytes(&mem, buffer(3), 8), ARMED);
    apply(&mut input, &mut platform, &mut mem, "KEY up:left");
    assert_eq!(read_bytes(&mem, buffer(3), 8), [0; 8]);
    // A release of a key that is not down is ignored.
    apply(&mut input, &mut platform, &mut mem, "KEY up:left");
    apply(&mut input, &mut platform, &mut mem, "KEY up:ctrl+right");
    assert_eq!(read_bytes(&mem, buffer(4), 8), ARMED);
    // `KEY left` is still a whole tap.
    apply(&mut input, &mut platform, &mut mem, "KEY left");
    assert_eq!(read_bytes(&mem, buffer(4), 8), [0, 0, 0x50, 0, 0, 0, 0, 0]);
    assert_eq!(read_bytes(&mem, buffer(5), 8), [0; 8]);
}

#[test]
fn at_most_six_keys_are_held_and_a_seventh_is_rejected() {
    let (mut platform, mut mem) = keyboard(8);
    let mut input = controller();
    for key in ["f1", "f2", "f3", "f4", "f5", "ctrl+left"] {
        apply(&mut input, &mut platform, &mut mem, &format!("KEY down:{key}"));
    }
    assert_eq!(read_bytes(&mem, buffer(5), 8), [0x01, 0, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0x50]);
    apply(&mut input, &mut platform, &mut mem, "KEY down:f6");
    assert_eq!(read_bytes(&mem, buffer(6), 8), ARMED, "the seventh key is not pressed");
    apply(&mut input, &mut platform, &mut mem, "KEY up:ctrl+left");
    assert_eq!(read_bytes(&mem, buffer(6), 8), [0, 0, 0x3a, 0x3b, 0x3c, 0x3d, 0x3e, 0]);
}
