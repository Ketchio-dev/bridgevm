use super::boot_key::XhciHidBootKeyTrigger;
use super::test_support::{emit_uart, new_platform};

#[test]
fn xhci_hid_input_does_not_fire_before_marker() {
    let mut platform = new_platform();
    let mut trigger = XhciHidBootKeyTrigger::from_env_value("cd-prompt", " ").unwrap();

    trigger.maybe_fire(&mut platform);

    assert!(!trigger.fired());
    assert_eq!(
        platform
            .xhci_hid_boot_key_report_stats()
            .queued_space_reports,
        0
    );
}

#[test]
fn xhci_hid_input_fires_once_after_marker() {
    let mut platform = new_platform();
    let mut trigger = XhciHidBootKeyTrigger::from_env_value("cd-prompt", " ").unwrap();
    emit_uart(&mut platform, b"Press any key to boot from CD or DVD");

    trigger.maybe_fire(&mut platform);
    trigger.maybe_fire(&mut platform);

    assert!(trigger.fired());
    assert_eq!(trigger.usage(), 0x2c);
    assert_eq!(
        platform
            .xhci_hid_boot_key_report_stats()
            .queued_space_reports,
        1
    );
}

#[test]
fn xhci_hid_input_rejects_empty_env() {
    assert!(XhciHidBootKeyTrigger::from_env_value("cd-prompt", "").is_none());
}

#[test]
fn xhci_hid_input_rejects_non_space_value() {
    assert!(XhciHidBootKeyTrigger::from_env_value("cd-prompt", "x").is_none());
}

#[test]
fn xhci_hid_input_serial_marker_does_not_fire_before_marker() {
    let mut platform = new_platform();
    let mut trigger = XhciHidBootKeyTrigger::from_env_value_with_marker(
        "serial-marker",
        " ",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();

    trigger.maybe_fire(&mut platform);

    assert!(!trigger.fired());
    assert_eq!(trigger.marker(), b"BdsDxe: starting Boot0001");
    assert_eq!(
        platform
            .xhci_hid_boot_key_report_stats()
            .queued_space_reports,
        0
    );
}

#[test]
fn xhci_hid_input_serial_marker_fires_once_after_marker() {
    let mut platform = new_platform();
    let mut trigger = XhciHidBootKeyTrigger::from_env_value_with_marker(
        "serial-marker",
        " ",
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();
    emit_uart(&mut platform, b"BdsDxe: starting Boot0001");

    trigger.maybe_fire(&mut platform);
    trigger.maybe_fire(&mut platform);

    assert!(trigger.fired());
    assert_eq!(trigger.usage(), 0x2c);
    assert_eq!(trigger.marker(), b"BdsDxe: starting Boot0001");
    assert_eq!(
        platform
            .xhci_hid_boot_key_report_stats()
            .queued_space_reports,
        1
    );
}

#[test]
fn xhci_hid_input_serial_marker_accepts_visual_space_sentinel() {
    // Given: the live runner prints and may pass the visual sentinel for a literal Space key.
    let value = "<space>";

    // When: the boot-key trigger is built from that value.
    let trigger = XhciHidBootKeyTrigger::from_env_value_with_marker(
        "serial-marker",
        value,
        b"BdsDxe: starting Boot0001",
    )
    .unwrap();

    // Then: the boot-key trigger is present instead of silently disappearing.
    assert_eq!(trigger.usage(), 0x2c);
    assert_eq!(trigger.marker(), b"BdsDxe: starting Boot0001");
}

#[test]
fn xhci_hid_input_rejects_non_space_serial_marker_value() {
    assert!(XhciHidBootKeyTrigger::from_env_value_with_marker(
        "serial-marker",
        "x",
        b"BdsDxe: starting Boot0001",
    )
    .is_none());
}
