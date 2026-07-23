//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtioBlockProbeCompletion {
    pub(crate) completion: VirtioBlockRequestCompletion,
    pub(crate) backing_kind: &'static str,
    pub(crate) byte_offset: u64,
    pub(crate) used_len: u32,
    pub(crate) data_prefix: Vec<u8>,
    pub(crate) status: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtioBlockWritableProbeCompletion {
    pub(crate) initial_read: VirtioBlockProbeCompletion,
    pub(crate) write_completion: VirtioBlockRequestCompletion,
    pub(crate) write_byte_offset: u64,
    pub(crate) write_used_len: u32,
    pub(crate) write_data_prefix: Vec<u8>,
    pub(crate) write_status: u8,
    pub(crate) flush_completion: VirtioBlockRequestCompletion,
    pub(crate) flush_used_len: u32,
    pub(crate) flush_status: u8,
    pub(crate) persisted_data_prefix: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VirtioBlockQueueProbeCompletion {
    ReadOnly(VirtioBlockProbeCompletion),
    Writable(VirtioBlockWritableProbeCompletion),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WindowsArmFirmwareBlockQueueCompletion {
    pub(crate) role: &'static str,
    pub(crate) backing_kind: &'static str,
    pub(crate) base_ipa: u64,
    pub(crate) byte_offset: u64,
    pub(crate) completion: VirtioBlockRequestCompletion,
    pub(crate) used_len: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VirtioBlockProbeBackingRef<'a> {
    Synthetic,
    HostFile(&'a PathBuf),
    HostIsoReadOnly(&'a PathBuf),
    HostFileWritable(&'a PathBuf),
}

impl<'a> VirtioBlockProbeBackingRef<'a> {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            Self::Synthetic => "synthetic-sector-pattern",
            Self::HostFile(_) => "host-file",
            Self::HostIsoReadOnly(_) => "host-iso-readonly",
            Self::HostFileWritable(_) => "host-file-writable",
        }
    }

    pub(crate) fn path(&self) -> Option<&'a PathBuf> {
        match self {
            Self::Synthetic => None,
            Self::HostFile(path) | Self::HostIsoReadOnly(path) | Self::HostFileWritable(path) => {
                Some(path)
            }
        }
    }
}

pub(crate) fn windows_arm_firmware_block_device_for_mmio_ipa(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    ipa: u64,
) -> Option<&WindowsArmVirtioBlockDeviceMetadata> {
    block_devices
        .iter()
        .find(|device| ipa >= device.base_ipa && ipa < device.base_ipa.saturating_add(device.bytes))
}

pub(crate) fn windows_arm_firmware_block_device_backing_ref(
    device: &WindowsArmVirtioBlockDeviceMetadata,
) -> Result<VirtioBlockProbeBackingRef<'_>, VirtioBlockRequestError> {
    let path =
        device
            .backing_path
            .as_ref()
            .ok_or(VirtioBlockRequestError::MissingBlockBackingPath {
                role: device.role,
                backing_kind: device.backing_kind,
            })?;
    match device.backing_kind {
        "host-iso-readonly" => Ok(VirtioBlockProbeBackingRef::HostIsoReadOnly(path)),
        "host-file-writable" => Ok(VirtioBlockProbeBackingRef::HostFileWritable(path)),
        "host-file" => Ok(VirtioBlockProbeBackingRef::HostFile(path)),
        backing_kind => Err(VirtioBlockRequestError::UnsupportedBlockBackingKind {
            role: device.role,
            backing_kind,
        }),
    }
}

pub(crate) fn windows_arm_firmware_block_queue_notify_ipa(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    ipa: u64,
) -> bool {
    windows_arm_firmware_block_device_for_mmio_ipa(block_devices, ipa).is_some_and(|device| {
        ipa.saturating_sub(device.base_ipa) == VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum WindowsArmFirmwareMmioDeviceKind {
    Pl011,
    Pl031,
    GicDistributor,
    GicRedistributor,
    VirtioInstallerIso,
    VirtioTargetDisk,
}

pub(crate) fn windows_arm_firmware_mmio_device_kind_label(
    kind: Option<WindowsArmFirmwareMmioDeviceKind>,
) -> &'static str {
    match kind {
        Some(WindowsArmFirmwareMmioDeviceKind::Pl011) => "pl011",
        Some(WindowsArmFirmwareMmioDeviceKind::Pl031) => "pl031",
        Some(WindowsArmFirmwareMmioDeviceKind::GicDistributor) => "gicd",
        Some(WindowsArmFirmwareMmioDeviceKind::GicRedistributor) => "gicr",
        Some(WindowsArmFirmwareMmioDeviceKind::VirtioInstallerIso) => "virtio-installer-iso",
        Some(WindowsArmFirmwareMmioDeviceKind::VirtioTargetDisk) => "virtio-target-disk",
        None => "unclassified",
    }
}

pub(crate) fn windows_arm_firmware_fixed_mmio_range_contains(
    ipa: u64,
    base_ipa: u64,
    bytes: u64,
) -> bool {
    ipa >= base_ipa && ipa < base_ipa.saturating_add(bytes)
}

pub(crate) fn windows_arm_firmware_mmio_device_kind(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    ipa: u64,
) -> Option<WindowsArmFirmwareMmioDeviceKind> {
    if windows_arm_firmware_fixed_mmio_range_contains(
        ipa,
        WINDOWS_ARM_PL011_MMIO_IPA,
        PL011_REGISTER_WINDOW_BYTES,
    ) {
        return Some(WindowsArmFirmwareMmioDeviceKind::Pl011);
    }
    if windows_arm_firmware_fixed_mmio_range_contains(
        ipa,
        WINDOWS_ARM_PL031_MMIO_IPA,
        PL031_REGISTER_WINDOW_BYTES,
    ) {
        return Some(WindowsArmFirmwareMmioDeviceKind::Pl031);
    }
    if windows_arm_firmware_fixed_mmio_range_contains(
        ipa,
        WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA,
        WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES,
    ) {
        return Some(WindowsArmFirmwareMmioDeviceKind::GicDistributor);
    }
    if windows_arm_firmware_fixed_mmio_range_contains(
        ipa,
        WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA,
        WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES,
    ) {
        return Some(WindowsArmFirmwareMmioDeviceKind::GicRedistributor);
    }

    windows_arm_firmware_block_device_for_mmio_ipa(block_devices, ipa).and_then(|device| {
        match device.role {
            "installer-iso" => Some(WindowsArmFirmwareMmioDeviceKind::VirtioInstallerIso),
            "target-disk" => Some(WindowsArmFirmwareMmioDeviceKind::VirtioTargetDisk),
            _ => None,
        }
    })
}
