//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

pub(crate) fn complete_windows_arm_firmware_block_queue_notify(
    bus: &mut MmioBus,
    memory: &mut VirtioGuestMemory<'_>,
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    ipa: u64,
    notify_value: u64,
) -> Result<WindowsArmFirmwareBlockQueueCompletion, VirtioBlockRequestError> {
    let device = windows_arm_firmware_block_device_for_mmio_ipa(block_devices, ipa)
        .ok_or(VirtioBlockRequestError::MissingBlockDeviceMetadata { ipa })?;
    if ipa.saturating_sub(device.base_ipa) != VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET {
        return Err(VirtioBlockRequestError::UnexpectedQueueNotifyIpa {
            role: device.role,
            ipa,
        });
    }
    if notify_value != VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE {
        return Err(VirtioBlockRequestError::UnsupportedQueueNotifyValue {
            role: device.role,
            value: notify_value,
        });
    }
    let backing = windows_arm_firmware_block_device_backing_ref(device)?;
    let block = bus.find_device_mut_at::<VirtioMmioBlockDevice>(ipa).ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO firmware block"),
    )?;
    let (completion, backing_kind) = match backing {
        VirtioBlockProbeBackingRef::HostFile(path) => {
            let mut backend = FileBlockStorageBackend::open(path)?;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
        VirtioBlockProbeBackingRef::HostIsoReadOnly(path) => {
            let mut backend = ReadOnlyIsoBlockStorageBackend::open(path)?;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
        VirtioBlockProbeBackingRef::HostFileWritable(path) => {
            let mut backend = WritableHostFileBlockStorageBackend::open(path)?;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
        VirtioBlockProbeBackingRef::Synthetic => {
            let mut backend = SyntheticBlockStorageBackend;
            let backing_kind = backend.kind();
            let completion =
                block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
            (completion, backing_kind)
        }
    };
    let byte_offset = completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: completion.sector,
        })?;
    let queue_size = u16::try_from(block.queue_num)
        .ok()
        .filter(|value| *value > 0)
        .ok_or(VirtioBlockRequestError::InvalidQueueSize(block.queue_num))?;
    let used_slot = u64::from(completion.used_index.wrapping_sub(1) % queue_size);
    let used_len = memory.read_u32(block.queue_device + 4 + (used_slot * 8) + 4)?;

    Ok(WindowsArmFirmwareBlockQueueCompletion {
        role: device.role,
        backing_kind,
        base_ipa: device.base_ipa,
        byte_offset,
        used_len,
        completion,
    })
}

pub(crate) fn windows_arm_firmware_block_device_spi(
    device: &WindowsArmVirtioBlockDeviceMetadata,
) -> Option<u32> {
    match device.base_ipa {
        WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA => Some(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI),
        WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA => Some(WINDOWS_ARM_VIRTIO_TARGET_DISK_SPI),
        _ => None,
    }
}

pub(crate) fn windows_arm_firmware_block_device_mmio_offset(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    ipa: u64,
) -> Option<u64> {
    block_devices.iter().find_map(|device| {
        let end = device.base_ipa.checked_add(device.bytes)?;
        (ipa >= device.base_ipa && ipa < end).then_some(ipa - device.base_ipa)
    })
}

pub(crate) fn windows_arm_firmware_block_irq_source_may_change(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    ipa: u64,
    value: u64,
) -> bool {
    matches!(
        windows_arm_firmware_block_device_mmio_offset(block_devices, ipa),
        Some(VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET | VIRTIO_MMIO_INTERRUPT_ACK_OFFSET)
    ) || matches!(
        windows_arm_firmware_block_device_mmio_offset(block_devices, ipa),
        Some(VIRTIO_MMIO_STATUS_OFFSET)
    ) && value == 0
}

pub(crate) fn windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
    ipa: u64,
    value: u64,
    width: u8,
) -> bool {
    let Some(offset) = ipa.checked_sub(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA) else {
        return false;
    };
    let pending_clear_bytes = ((GICV3_SUPPORTED_INTERRUPT_COUNT / 32) as u64) * 4;
    offset >= GICD_ICPENDR_BASE_OFFSET
        && offset < GICD_ICPENDR_BASE_OFFSET + pending_clear_bytes
        && mask_mmio_value(value, width) != 0
}

#[cfg(test)]
pub(crate) fn windows_arm_firmware_device_irq_line_assertable(
    bus: &mut MmioBus,
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
) -> bool {
    let spis: Vec<u32> = block_devices
        .iter()
        .filter_map(windows_arm_firmware_block_device_spi)
        .collect();

    let Some(gicd) =
        bus.find_device_mut_at::<GicV3DistributorDevice>(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
    else {
        return false;
    };

    spis.into_iter()
        .any(|spi| gicd.spi_irq_line_assertable(spi))
}

pub(crate) fn refresh_windows_arm_firmware_device_irq_pending(
    bus: &mut MmioBus,
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
) -> bool {
    let interrupt_states: Vec<(u32, bool)> = block_devices
        .iter()
        .filter_map(|device| {
            let spi = windows_arm_firmware_block_device_spi(device)?;
            let pending = bus
                .find_device_mut_at::<VirtioMmioBlockDevice>(device.base_ipa)
                .is_some_and(|block| {
                    (block.interrupt_status & VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE) != 0
                });
            Some((spi, pending))
        })
        .collect();

    let Some(gicd) =
        bus.find_device_mut_at::<GicV3DistributorDevice>(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
    else {
        return false;
    };

    let mut irq_line_assertable = false;
    for (spi, pending) in interrupt_states {
        let _ = gicd.set_spi_pending(spi, pending);
        irq_line_assertable |= gicd.spi_irq_line_assertable(spi);
    }
    irq_line_assertable
}

pub(crate) fn acknowledge_windows_arm_firmware_gic_irq(
    bus: &mut MmioBus,
    priority_mask: u8,
) -> Option<GicV3PendingInterrupt> {
    let redistributor_pending = bus
        .find_device_mut_at::<GicV3RedistributorDevice>(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
        .and_then(|gicr| gicr.pending_interrupt_for_cpu(priority_mask));
    let distributor_pending = bus
        .find_device_mut_at::<GicV3DistributorDevice>(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
        .and_then(|gicd| gicd.pending_interrupt_for_cpu(priority_mask));

    let interrupt = select_highest_priority_interrupt(redistributor_pending, distributor_pending)?;

    if interrupt.interrupt_id < 32 {
        if bus
            .find_device_mut_at::<GicV3RedistributorDevice>(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
            .is_some_and(|gicr| gicr.acknowledge_interrupt_id(interrupt.interrupt_id))
        {
            return Some(interrupt);
        }
        return None;
    }

    if bus
        .find_device_mut_at::<GicV3DistributorDevice>(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
        .is_some_and(|gicd| gicd.acknowledge_interrupt_id(interrupt.interrupt_id))
    {
        Some(interrupt)
    } else {
        None
    }
}

pub(crate) fn end_windows_arm_firmware_gic_irq(bus: &mut MmioBus, interrupt_id: u32) -> bool {
    if interrupt_id < 32 {
        return bus
            .find_device_mut_at::<GicV3RedistributorDevice>(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
            .is_some_and(|gicr| gicr.end_interrupt(interrupt_id));
    }

    bus.find_device_mut_at::<GicV3DistributorDevice>(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
        .is_some_and(|gicd| gicd.end_interrupt(interrupt_id))
}

pub(crate) fn pending_windows_arm_firmware_gic_irq(bus: &mut MmioBus, priority_mask: u8) -> u32 {
    let redistributor_pending = bus
        .find_device_mut_at::<GicV3RedistributorDevice>(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
        .and_then(|gicr| gicr.pending_interrupt_for_cpu(priority_mask));
    let distributor_pending = bus
        .find_device_mut_at::<GicV3DistributorDevice>(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA)
        .and_then(|gicd| gicd.pending_interrupt_for_cpu(priority_mask));

    select_highest_priority_interrupt(redistributor_pending, distributor_pending)
        .map(|interrupt| interrupt.interrupt_id)
        .unwrap_or(GICV3_SPURIOUS_INTERRUPT_ID)
}

pub(crate) fn set_windows_arm_firmware_vtimer_ppi_pending(
    bus: &mut MmioBus,
    pending: bool,
) -> bool {
    if GicV3RedistributorDevice::fdt_ppi_interrupt_id(WINDOWS_ARM_VIRTUAL_TIMER_PPI)
        != Some(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID)
    {
        return false;
    }

    bus.find_device_mut_at::<GicV3RedistributorDevice>(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA)
        .is_some_and(|gicr| gicr.set_fdt_ppi_pending(WINDOWS_ARM_VIRTUAL_TIMER_PPI, pending))
}
