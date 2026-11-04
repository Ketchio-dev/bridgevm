//! The firmware's preferred resolution and the Windows desktop switch.

use super::*;
use bridgevm_hvf::uefi_resolution::with_resolution;

/// An empty EDK2 authenticated variable store: FV header, store header, erased space.
fn empty_vars() -> Vec<u8> {
    let mut vars = vec![0xff; 0x2000];
    vars[..0x64].fill(0);
    vars[0x28..0x2c].copy_from_slice(b"_FVH");
    vars[0x30..0x32].copy_from_slice(&0x48u16.to_le_bytes());
    vars[0x48..0x58].copy_from_slice(&[
        0x78, 0x2c, 0xf3, 0xaa, 0x7b, 0x94, 0x9a, 0x43, 0xa1, 0x80, 0x2e, 0x14, 0x4e, 0xc3, 0x77,
        0x92,
    ]);
    vars[0x58..0x5c].copy_from_slice(&(0x2000u32 - 0x48).to_le_bytes());
    vars
}

#[test]
fn a_chosen_mode_is_switched_to_even_when_the_vars_already_ask_for_it() {
    // The firmware already asks for 1024x768 (set in its setup, or by a boot
    // that stopped before the switch ran); Windows may still show 800x600.
    let at_1024 = with_resolution(&empty_vars(), (1024, 768)).expect("an EDK2 store");
    assert_eq!(
        guest_resolution(Some("1024x768"), &at_1024, true),
        (None, Some((1024, 768)))
    );
    // A virtio-gpu display's desktop follows the window instead.
    assert_eq!(
        guest_resolution(Some("1024x768"), &at_1024, false),
        (None, None)
    );
    // A new choice is written and switched to on either display.
    let (vars, desktop) = guest_resolution(Some("1920x1080"), &at_1024, false);
    assert_eq!(vars, with_resolution(&at_1024, (1920, 1080)));
    assert_eq!(desktop, Some((1920, 1080)));
    // The default never replaces a preference, nor switches without adding one.
    assert_eq!(guest_resolution(None, &at_1024, true), (None, None));
}

#[test]
fn nvme_reads_and_writes_keep_the_platform_lock_unless_opted_in() {
    assert!(!nvme_unlocked_io(None, false), "off by default");
    assert!(!nvme_unlocked_io(Some("0"), false));
    assert!(!nvme_unlocked_io(Some(""), false));
    assert!(nvme_unlocked_io(Some("1"), false), "=1 opts in");
    assert!(!nvme_unlocked_io(Some("1"), true), "checkpoints keep I/O inline");
}
