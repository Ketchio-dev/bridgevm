//! Firmware block-device MMIO offset lookup for IPAs outside every device.

use crate::probe_mmio::*;
use crate::*;

#[test]
fn firmware_block_offset_ignores_mmio_below_every_block_device() {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    assert!(block_devices
        .iter()
        .all(|device| device.base_ipa > WINDOWS_ARM_PL011_MMIO_IPA));
    for ipa in [WINDOWS_ARM_PL011_MMIO_IPA, WINDOWS_ARM_PL031_MMIO_IPA] {
        assert_eq!(
            windows_arm_firmware_block_device_mmio_offset(&block_devices, ipa),
            None
        );
        assert!(!windows_arm_firmware_block_irq_source_may_change(
            &block_devices,
            ipa,
            0
        ));
    }
}
