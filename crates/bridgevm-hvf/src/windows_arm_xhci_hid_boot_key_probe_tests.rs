use super::*;

#[test]
fn windows_11_arm_xhci_hid_boot_key_report_probe_is_metadata_safe() {
    let probe = probe_windows_11_arm_xhci_hid_boot_key_report();
    let output = probe.render_text();
    println!("{output}");

    assert!(!probe.qemu_used);
    assert!(!probe.apple_vz_used);
    assert!(!probe.hvf_entered);
    assert!(!probe.windows_boot_claimed);
    assert_eq!(probe.usage_page, 0x07);
    assert_eq!(probe.usage_id, 0x2c);
    assert_eq!(probe.key_report, [0, 0, 0x2c, 0, 0, 0, 0, 0]);
    assert_eq!(probe.release_report, [0; 8]);
    assert_eq!(probe.transfer_events, 2);
    assert!(probe.blockers.is_empty());
    assert!(output.contains("Windows 11 Arm HVF xHCI HID boot-key report probe"));
    assert!(output.contains("QEMU: not used"));
    assert!(output.contains("Apple VZ: not used"));
    assert!(output.contains("HVF: not entered"));
    assert!(output.contains("Windows boot: not claimed"));
    assert!(output.contains("Usage page: 0x07"));
    assert!(output.contains("Usage ID: 0x2c"));
    assert!(output.contains("Key report: 00 00 2c 00 00 00 00 00"));
    assert!(output.contains("Release report: 00 00 00 00 00 00 00 00"));
    assert!(output.contains("Transfer events: 2"));
    assert!(output.contains("Blockers: none"));
    assert!(!output.contains("qemu-system"));
    assert!(!output.contains('%'));
}
