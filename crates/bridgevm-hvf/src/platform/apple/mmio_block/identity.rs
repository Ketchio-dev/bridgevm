//! Split out of mmio_block.rs by responsibility.

use super::super::*;
use crate::*;

pub(crate) fn block_identity_register_specs() -> [BlockIdentityRegisterSpec; 4] {
    [
        BlockIdentityRegisterSpec {
            name: "magic",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
            value: VIRTIO_MMIO_MAGIC_VALUE,
            address_reg: HV_REG_X1,
            instruction: AARCH64_LDR_W0_FROM_X1,
        },
        BlockIdentityRegisterSpec {
            name: "version",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_VERSION_OFFSET,
            value: VIRTIO_MMIO_VERSION_VALUE,
            address_reg: HV_REG_X2,
            instruction: AARCH64_LDR_W0_FROM_X2,
        },
        BlockIdentityRegisterSpec {
            name: "device_id",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_DEVICE_ID_OFFSET,
            value: VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE,
            address_reg: HV_REG_X3,
            instruction: AARCH64_LDR_W0_FROM_X3,
        },
        BlockIdentityRegisterSpec {
            name: "vendor_id",
            ipa: BLOCK_MMIO_IPA + VIRTIO_MMIO_VENDOR_ID_OFFSET,
            value: VIRTIO_MMIO_VENDOR_ID_VALUE,
            address_reg: HV_REG_X4,
            instruction: AARCH64_LDR_W0_FROM_X4,
        },
    ]
}

pub(crate) fn block_register_probe_defaults() -> Vec<HvfMmioBlockRegisterProbe> {
    block_identity_register_specs()
        .iter()
        .map(|spec| HvfMmioBlockRegisterProbe {
            name: spec.name,
            ipa: spec.ipa,
            expected_value: spec.value,
            run_attempted: false,
            exit_observed: false,
            handled_by_device: false,
            value_injected: false,
            pc_read_after_exit: false,
            pc_advanced: false,
            run_status: None,
            exit_reason: None,
            exit_syndrome: None,
            exit_virtual_address: None,
            exit_physical_address: None,
            watchdog_cancel_status: None,
            value_set_status: None,
            pc_read_status: None,
            pc_after_exit: None,
            pc_advance_status: None,
        })
        .collect()
}
