//! USBCMD.INTE gates notification independently of IMAN.IE and PCI MSI-X masks.
#[path = "xhci_pending_enable/support.rs"]
mod support;
use support::*;

#[test]
fn global_disable_retains_completion_until_reenabled() {
    let mut x = Fixture::new(true, false, true);
    x.write(0x40, 4, 1); // RS remains set; clear only INTE.
    assert_eq!(x.read(0x40), 1);
    x.complete(0);
    assert!(x.messages().is_empty(), "INTE=0 must block MSI-X requests");
    assert_eq!(x.read(IMAN), 3);
    assert_eq!(x.read(PBA), 0);
    x.write(0x40, 4, 5);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    x.write(0x40, 4, 5);
    assert!(x.messages().is_empty());
}

#[test]
fn global_disable_does_not_arm_masked_vector() {
    let mut x = Fixture::new(true, true, true);
    x.write(0x40, 4, 1);
    x.complete(0);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 0, "global gate precedes PCI vector masking");
    assert_eq!(x.read(IMAN), 3);
    x.write(0x40, 4, 5);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 1);
    x.write(TABLE + 12, 4, 0);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(PBA), 0);
    assert_eq!(x.read(IMAN), 2);
}

#[test]
fn global_disable_does_not_arm_function_masked_notification() {
    let mut x = Fixture::new(true, false, true);
    x.configure_msix(true, true);
    x.write(0x40, 4, 1);
    x.complete(0);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 0);
    x.write(0x40, 4, 5);
    assert_eq!(x.read(PBA), 1);
    assert!(x.messages().is_empty());
    x.configure_msix(true, false);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(PBA), 0);
}

#[test]
fn disable_after_delivery_stops_new_event_and_ack_prevents_replay() {
    let mut x = Fixture::new(true, false, true);
    x.complete(0);
    assert_eq!(x.messages(), vec![message()]);
    x.write(0x40, 4, 1);
    x.complete(1);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(IMAN), 3);
    x.write(IMAN, 4, 3); // Explicit W1C while globally disabled.
    assert_eq!(x.read(IMAN), 2);
    x.write(0x40, 4, 5);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 0);
}

#[test]
fn enabled_control_and_individual_disable_keep_existing_behavior() {
    let mut x = Fixture::new(true, false, true);
    x.complete(0);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    x.write(IMAN, 4, 0);
    x.complete(1);
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 0);
    x.write(IMAN, 4, 2);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
}

#[test]
fn pci_pending_message_survives_global_disable() {
    let mut x = Fixture::new(true, true, true);
    x.complete(0); // Request already reached PCI while INTE was enabled.
    assert!(x.messages().is_empty());
    assert_eq!(x.read(PBA), 1);
    x.write(0x40, 4, 1);
    assert_eq!(x.read(PBA), 1);
    x.write(TABLE + 12, 4, 0);
    assert_eq!(x.messages(), vec![message()]);
    assert_eq!(x.read(IMAN), 2);
    assert_eq!(x.read(PBA), 0);
    x.write(0x40, 4, 5);
    assert!(x.messages().is_empty());
}

#[test]
fn reset_discards_globally_disabled_pending_event() {
    let mut x = Fixture::new(false, false, true);
    x.write(0x40, 4, 1);
    x.complete(0);
    assert_eq!(x.read(IMAN), 1);
    x.write(0x40, 4, 0);
    assert_eq!(x.read(0x44) & 1, 1);
    x.write(0x40, 4, 2);
    assert_eq!(x.read(0x40) & 4, 0);
    assert_eq!(x.read(IMAN), 0);
    x.write(0x40, 4, 4); // Enable globally while halted, with no pending event.
    x.write(IMAN, 4, 2);
    assert!(x.messages().is_empty());
}
