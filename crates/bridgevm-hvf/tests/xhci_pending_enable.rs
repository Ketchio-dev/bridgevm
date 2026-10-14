//! Enabling a pending interrupter must notify without another command completion.
#[path = "xhci_pending_enable/support.rs"]
mod support;
use support::*;

#[test]
fn enabling_pending_iman_delivers_once_without_new_command() {
    let mut x = Fixture::new(false, false, true);
    x.complete(0);
    assert_eq!(x.read(IMAN), 1);
    assert_eq!(x.read(PBA), 0);
    assert!(x.messages().is_empty());
    x.write(IMAN, 4, 2);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    assert_eq!(x.read(PBA), 0);
    x.write(IMAN, 4, 2);
    assert!(x.messages().is_empty());
    x.complete(1);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
}

#[test]
fn enabling_msix_delivers_existing_enabled_interrupter() {
    let mut x = Fixture::new(true, false, false);
    x.complete(0);
    assert_eq!(x.read(IMAN), 3);
    assert_eq!(x.read(PBA), 0);
    assert!(x.messages().is_empty());
    x.configure_msix(true, false);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    assert_eq!(x.read(PBA), 0);
    x.configure_msix(true, false);
    assert!(x.messages().is_empty());
}

#[test]
fn enabling_iman_while_vector_masked_arms_pba_until_unmask() {
    let mut x = Fixture::new(false, true, true);
    x.complete(0);
    assert_eq!(x.read(PBA), 0);
    x.write(IMAN, 4, 2);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(IMAN), 3);
    assert_eq!(x.read(PBA), 1);
    x.write(TABLE + 12, 4, 0);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    assert_eq!(x.read(PBA), 0);
}

#[test]
fn pending_enable_respects_function_mask_until_ecam_unmask() {
    let mut x = Fixture::new(false, false, true);
    x.configure_msix(true, true);
    x.complete(0);
    assert_eq!(x.read(PBA), 0);
    x.write(IMAN, 4, 2);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(IMAN), 3);
    assert_eq!(x.read(PBA), 1);
    x.configure_msix(true, false);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    assert_eq!(x.read(PBA), 0);
    x.configure_msix(true, false);
    assert!(x.messages().is_empty());
}

#[test]
fn disabled_or_acknowledged_interrupter_does_not_notify() {
    let mut x = Fixture::new(false, false, true);
    x.complete(0);
    x.write(IMAN, 4, 0);
    assert_eq!(x.read(IMAN), 1);
    assert!(x.messages().is_empty());
    x.write(IMAN, 4, 3); // enable plus W1C acknowledgement, no pending event
    assert_eq!(x.read(IMAN), 2);
    assert_eq!(x.read(PBA), 0);
    assert!(x.messages().is_empty());
}

#[test]
fn enabled_completion_and_pending_mask_replay_stay_single() {
    let mut x = Fixture::new(true, false, true);
    x.complete(0);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    x.write(TABLE + 12, 4, 1);
    x.complete(1);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 1);
    x.write(TABLE + 12, 4, 0);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(PBA), 0);
    x.write(TABLE + 12, 4, 0);
    assert!(x.messages().is_empty());
}

#[test]
fn controller_reset_discards_disabled_pending_event() {
    let mut x = Fixture::new(false, false, true);
    x.complete(0);
    assert_eq!(x.read(IMAN), 1);
    x.write(0x40, 4, 0); // Halt before HCRST, as required by xHCI 5.4.1.
    assert_eq!(x.read(0x44) & 1, 1);
    x.write(0x40, 4, 2); // HCRST
    assert_eq!(x.read(IMAN), 0);
    assert_eq!(x.read(PBA), 0);
    x.write(IMAN, 4, 2);
    assert!(x.messages().is_empty());
}
