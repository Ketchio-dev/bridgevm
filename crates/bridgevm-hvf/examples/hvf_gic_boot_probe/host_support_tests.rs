use super::*;

#[test]
fn a_disk_with_nothing_to_start_is_reported_once() {
    // As the firmware printed it for a blank disk.
    let mut serial = b"BdsDxe: failed to load Boot0003 \"Windows Boot Manager\" from HD(1,GPT,..)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi: Not Found\r\n"
        .to_vec();
    let mut scan = NoBootableDisk::default();
    assert_eq!(scan.line(&serial), None);
    serial.extend_from_slice(b"BdsDxe: No bootable option or device was found.\r\nBdsDxe: Press any key");
    assert_eq!(scan.line(&serial), Some("BVBOOT no-bootable-disk"));
    assert_eq!(scan.line(&serial), None, "once");
}

#[test]
fn a_boot_loader_for_another_processor_is_named() {
    let mut scan = NoBootableDisk::default();
    let mut serial = b"BdsDxe: failed to load Boot0001 \"Windows Boot Manager\" from HD(1,GPT,..)/\\EFI\\Microsoft\\Boot\\bootmgfw.efi: Unsupp".to_vec();
    assert_eq!(scan.line(&serial), None);
    // The needle spans two reads.
    serial.extend_from_slice(b"orted\r\nBdsDxe: No bootable option or device was found.\r\n");
    assert_eq!(scan.line(&serial), Some("BVBOOT no-bootable-disk loader=other-cpu"));
}
