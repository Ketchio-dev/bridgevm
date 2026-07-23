//! Split out of probe_mmio.rs by responsibility.

use super::*;
use crate::*;

#[derive(Default)]
pub(crate) struct MmioBus {
    pub(crate) devices: Vec<Box<dyn MmioDevice>>,
}

impl MmioBus {
    pub(crate) fn attach(&mut self, device: Box<dyn MmioDevice>) {
        self.devices.push(device);
    }

    pub(crate) fn device_count(&self) -> usize {
        self.devices.len()
    }

    pub(crate) fn dispatch(&mut self, access: MmioAccess) -> MmioAction {
        self.devices
            .iter_mut()
            .find(|device| device.range().contains(access.ipa))
            .map_or(MmioAction::Unhandled, |device| device.handle(access))
    }

    pub(crate) fn find_device_mut<T: 'static>(&mut self) -> Option<&mut T> {
        self.devices
            .iter_mut()
            .find_map(|device| device.as_any_mut().downcast_mut::<T>())
    }

    pub(crate) fn find_device_mut_at<T: 'static>(&mut self, ipa: u64) -> Option<&mut T> {
        self.devices
            .iter_mut()
            .filter(|device| device.range().contains(ipa))
            .find_map(|device| device.as_any_mut().downcast_mut::<T>())
    }
}

#[cfg(test)]
pub(crate) fn windows_arm_firmware_mmio_bus() -> MmioBus {
    let block_devices = windows_arm_firmware_block_devices(None, None);
    windows_arm_firmware_mmio_bus_with_block_devices(&block_devices)
}

pub(crate) fn windows_arm_firmware_mmio_bus_with_block_devices(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
) -> MmioBus {
    let mut bus = MmioBus::default();
    bus.attach(Box::new(Pl011UartDevice::new(
        WINDOWS_ARM_PL011_MMIO_IPA,
        WINDOWS_ARM_PL011_FLAG_VALUE,
    )));
    bus.attach(Box::new(Pl031RtcDevice::new(
        WINDOWS_ARM_PL031_MMIO_IPA,
        WINDOWS_ARM_PL031_READ_VALUE,
    )));
    bus.attach(Box::new(GicV3DistributorDevice::new(
        WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA,
    )));
    bus.attach(Box::new(GicV3RedistributorDevice::new(
        WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA,
    )));
    for block_device in block_devices {
        bus.attach(Box::new(VirtioMmioBlockDevice::from_metadata(block_device)));
    }
    bus
}

pub(crate) fn windows_arm_device_mmio_contains(ipa: u64) -> bool {
    ipa >= WINDOWS_ARM_DEVICE_MMIO_IPA
        && ipa < WINDOWS_ARM_DEVICE_MMIO_IPA.saturating_add(WINDOWS_ARM_DEVICE_MMIO_BYTES)
}

pub(crate) fn mask_mmio_value(value: u64, width: u8) -> u64 {
    if width >= 8 {
        value
    } else {
        value & ((1_u64 << (u64::from(width) * 8)) - 1)
    }
}
