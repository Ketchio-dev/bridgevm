//! Synthetic MMIO harness used by the diagnostic probes and the firmware
//! run loop: the MMIO bus/device abstraction, the PL011/PL031/GICv3 and
//! virtio-mmio-block *diagnostic* models (distinct from the real device
//! modules), the virtqueue/guest-memory primitives and storage backends, and
//! the low-vector post-repair telemetry.
//!
//! Moved verbatim out of the legacy probe monolith. Every item here was, and
//! remains, crate-internal: nothing in this module is part of the public API.

// The harness is tightly interwoven with the crate-root probe types, layout
// constants and decoders it was extracted from; import them wholesale rather
// than enumerating ~30 names. Nothing here is re-exported publicly.
use crate::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmioAccessKind {
    Read,
    Write,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MmioAccess {
    pub(crate) ipa: u64,
    pub(crate) kind: MmioAccessKind,
    pub(crate) value: Option<u64>,
    pub(crate) width: u8,
}

impl MmioAccess {
    pub(crate) fn read(ipa: u64, width: u8) -> Self {
        Self {
            ipa,
            kind: MmioAccessKind::Read,
            value: None,
            width,
        }
    }

    pub(crate) fn write(ipa: u64, value: u64, width: u8) -> Self {
        Self {
            ipa,
            kind: MmioAccessKind::Write,
            value: Some(value),
            width,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MmioAction {
    ReadValue(u64),
    WriteAccepted { value: u64, byte: u8 },
    Unhandled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MmioRange {
    pub(crate) start: u64,
    pub(crate) bytes: u64,
}

impl MmioRange {
    pub(crate) fn contains(&self, ipa: u64) -> bool {
        ipa >= self.start && ipa < self.start.saturating_add(self.bytes)
    }
}

pub(crate) trait MmioDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any;
    fn range(&self) -> MmioRange;
    fn handle(&mut self, access: MmioAccess) -> MmioAction;
}

pub(crate) const PL011_UART_MODEL: &str = "PL011 UART skeleton";
pub(crate) const PL011_DR_OFFSET: u64 = 0x00;
pub(crate) const PL011_FR_OFFSET: u64 = 0x18;
pub(crate) const PL011_REGISTER_WINDOW_BYTES: u64 = 0x1000;
pub(crate) const PL031_DR_OFFSET: u64 = 0x00;
pub(crate) const PL031_REGISTER_WINDOW_BYTES: u64 = 0x1000;
pub(crate) const GICD_CTLR_OFFSET: u64 = 0x000;
pub(crate) const GICD_TYPER_OFFSET: u64 = 0x004;
pub(crate) const GICD_IIDR_OFFSET: u64 = 0x008;
pub(crate) const GICD_STATUSR_OFFSET: u64 = 0x010;
pub(crate) const GICD_IGROUPR_BASE_OFFSET: u64 = 0x080;
pub(crate) const GICD_ISENABLER_BASE_OFFSET: u64 = 0x100;
pub(crate) const GICD_ICENABLER_BASE_OFFSET: u64 = 0x180;
pub(crate) const GICD_ISPENDR_BASE_OFFSET: u64 = 0x200;
pub(crate) const GICD_ICPENDR_BASE_OFFSET: u64 = 0x280;
pub(crate) const GICD_ISACTIVER_BASE_OFFSET: u64 = 0x300;
pub(crate) const GICD_ICACTIVER_BASE_OFFSET: u64 = 0x380;
pub(crate) const GICD_IPRIORITYR_BASE_OFFSET: u64 = 0x400;
pub(crate) const GICD_ICFGR_BASE_OFFSET: u64 = 0xc00;
pub(crate) const GICD_IGRPMODR_BASE_OFFSET: u64 = 0xd00;
pub(crate) const GICD_IROUTER_BASE_OFFSET: u64 = 0x6000;
pub(crate) const GICD_CTLR_ENABLE_GRP1NS: u32 = 1 << 1;
pub(crate) const GICR_CTLR_OFFSET: u64 = 0x0000;
pub(crate) const GICR_IIDR_OFFSET: u64 = 0x0004;
pub(crate) const GICR_TYPER_OFFSET: u64 = 0x0008;
pub(crate) const GICR_STATUSR_OFFSET: u64 = 0x0010;
pub(crate) const GICR_WAKER_OFFSET: u64 = 0x0014;
pub(crate) const GICR_PROPBASER_OFFSET: u64 = 0x0070;
pub(crate) const GICR_PENDBASER_OFFSET: u64 = 0x0078;
pub(crate) const GICR_SGI_BASE_OFFSET: u64 = 0x1_0000;
pub(crate) const GICR_SGI_IGROUPR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x080;
pub(crate) const GICR_SGI_ISENABLER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x100;
pub(crate) const GICR_SGI_ICENABLER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x180;
pub(crate) const GICR_SGI_ISPENDR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x200;
pub(crate) const GICR_SGI_ICPENDR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x280;
pub(crate) const GICR_SGI_ISACTIVER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x300;
pub(crate) const GICR_SGI_ICACTIVER0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x380;
pub(crate) const GICR_SGI_IPRIORITYR_BASE_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0x400;
pub(crate) const GICR_SGI_ICFGR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0xc00;
pub(crate) const GICR_SGI_IGRPMODR0_OFFSET: u64 = GICR_SGI_BASE_OFFSET + 0xd00;
pub(crate) const GICV3_SUPPORTED_INTERRUPT_COUNT: usize = 64;
pub(crate) const GICV3_INTERRUPT_REGISTER_COUNT: usize = GICV3_SUPPORTED_INTERRUPT_COUNT / 32;
pub(crate) const GICV3_PRIORITY_REGISTER_COUNT: usize = GICV3_SUPPORTED_INTERRUPT_COUNT / 4;
pub(crate) const GICV3_CONFIG_REGISTER_COUNT: usize = GICV3_SUPPORTED_INTERRUPT_COUNT / 16;
pub(crate) const GICV3_IIDR_VALUE: u64 = 0x4252_564d;
pub(crate) const GICD_TYPER_VALUE: u64 = 1 | (5 << 19);
pub(crate) const GICR_TYPER_VALUE: u64 = 1 << 4;
pub(crate) const GICV3_DEFAULT_PRIORITY_WORD: u32 = 0xa0a0_a0a0;
pub(crate) const GICR_WAKER_PROCESSOR_SLEEP: u64 = 1 << 1;
pub(crate) const GICR_WAKER_CHILDREN_ASLEEP: u64 = 1 << 2;
pub(crate) const WINDOWS_ARM_VIRTUAL_TIMER_PPI: u32 = 11;
pub(crate) const WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID: u32 = 16 + WINDOWS_ARM_VIRTUAL_TIMER_PPI;
pub(crate) const AARCH64_SYSREG_TRAP_EXCEPTION_CLASS: u64 = 0x18;
pub(crate) const ICC_PMR_EL1_SYSREG: u16 = 0xc230;
pub(crate) const ICC_IAR0_EL1_SYSREG: u16 = 0xc640;
pub(crate) const ICC_EOIR0_EL1_SYSREG: u16 = 0xc641;
pub(crate) const ICC_HPPIR0_EL1_SYSREG: u16 = 0xc642;
pub(crate) const ICC_BPR0_EL1_SYSREG: u16 = 0xc643;
pub(crate) const ICC_AP0R0_EL1_SYSREG: u16 = 0xc644;
pub(crate) const ICC_AP0R1_EL1_SYSREG: u16 = 0xc645;
pub(crate) const ICC_AP0R2_EL1_SYSREG: u16 = 0xc646;
pub(crate) const ICC_AP0R3_EL1_SYSREG: u16 = 0xc647;
pub(crate) const ICC_AP1R0_EL1_SYSREG: u16 = 0xc648;
pub(crate) const ICC_AP1R1_EL1_SYSREG: u16 = 0xc649;
pub(crate) const ICC_AP1R2_EL1_SYSREG: u16 = 0xc64a;
pub(crate) const ICC_AP1R3_EL1_SYSREG: u16 = 0xc64b;
pub(crate) const ICC_DIR_EL1_SYSREG: u16 = 0xc659;
pub(crate) const ICC_RPR_EL1_SYSREG: u16 = 0xc65b;
pub(crate) const ICC_SGI1R_EL1_SYSREG: u16 = 0xc65d;
pub(crate) const ICC_IAR1_EL1_SYSREG: u16 = 0xc660;
pub(crate) const ICC_EOIR1_EL1_SYSREG: u16 = 0xc661;
pub(crate) const ICC_HPPIR1_EL1_SYSREG: u16 = 0xc662;
pub(crate) const ICC_BPR1_EL1_SYSREG: u16 = 0xc663;
pub(crate) const ICC_CTLR_EL1_SYSREG: u16 = 0xc664;
pub(crate) const ICC_CTLR_EL1_EOIMODE: u64 = 1 << 1;
pub(crate) const ICC_SRE_EL1_SYSREG: u16 = 0xc665;
pub(crate) const ICC_IGRPEN0_EL1_SYSREG: u16 = 0xc666;
pub(crate) const ICC_IGRPEN1_EL1_SYSREG: u16 = 0xc667;
pub(crate) const GICV3_SPURIOUS_INTERRUPT_ID: u32 = 1023;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GicV3PendingInterrupt {
    pub(crate) interrupt_id: u32,
    pub(crate) priority: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GicV3ActiveInterrupt {
    pub(crate) interrupt_id: u32,
    pub(crate) priority: u8,
    pub(crate) priority_dropped: bool,
}

pub(crate) fn select_highest_priority_interrupt(
    first: Option<GicV3PendingInterrupt>,
    second: Option<GicV3PendingInterrupt>,
) -> Option<GicV3PendingInterrupt> {
    [first, second]
        .into_iter()
        .flatten()
        .min_by_key(|interrupt| (interrupt.priority, interrupt.interrupt_id))
}

pub(crate) const VIRTIO_MMIO_MAGIC_VALUE_OFFSET: u64 = 0x000;
pub(crate) const VIRTIO_MMIO_VERSION_OFFSET: u64 = 0x004;
pub(crate) const VIRTIO_MMIO_DEVICE_ID_OFFSET: u64 = 0x008;
pub(crate) const VIRTIO_MMIO_VENDOR_ID_OFFSET: u64 = 0x00c;
pub(crate) const VIRTIO_MMIO_DEVICE_FEATURES_OFFSET: u64 = 0x010;
pub(crate) const VIRTIO_MMIO_DRIVER_FEATURES_OFFSET: u64 = 0x020;
pub(crate) const VIRTIO_MMIO_QUEUE_SEL_OFFSET: u64 = 0x030;
pub(crate) const VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET: u64 = 0x034;
pub(crate) const VIRTIO_MMIO_QUEUE_NUM_OFFSET: u64 = 0x038;
pub(crate) const VIRTIO_MMIO_QUEUE_READY_OFFSET: u64 = 0x044;
pub(crate) const VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET: u64 = 0x050;
pub(crate) const VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET: u64 = 0x060;
pub(crate) const VIRTIO_MMIO_INTERRUPT_ACK_OFFSET: u64 = 0x064;
pub(crate) const VIRTIO_MMIO_STATUS_OFFSET: u64 = 0x070;
pub(crate) const VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET: u64 = 0x080;
pub(crate) const VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET: u64 = 0x084;
pub(crate) const VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET: u64 = 0x090;
pub(crate) const VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET: u64 = 0x094;
pub(crate) const VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET: u64 = 0x0a0;
pub(crate) const VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET: u64 = 0x0a4;
pub(crate) const VIRTIO_MMIO_CONFIG_GENERATION_OFFSET: u64 = 0x0fc;
pub(crate) const VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET: u64 = 0x100;
pub(crate) const VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET: u64 = 0x104;
pub(crate) const VIRTIO_MMIO_REGISTER_WINDOW_BYTES: u64 = 0x1000;
pub(crate) const VIRTIO_MMIO_MAGIC_VALUE: u64 = 0x7472_6976;
pub(crate) const VIRTIO_MMIO_VERSION_VALUE: u64 = 2;
pub(crate) const VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE: u64 = 2;
pub(crate) const VIRTIO_MMIO_VENDOR_ID_VALUE: u64 = 0x4252_564d;
pub(crate) const VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE: u64 = 0;
pub(crate) const VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE: u64 = 0;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE: u64 = 0;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE: u64 = 128;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE: u64 = 8;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE: u64 = 1;
pub(crate) const VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE: u64 = 0x1;
pub(crate) const VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE: u64 = 0x3;
pub(crate) const VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE: u64 = 0xb;
pub(crate) const VIRTIO_MMIO_BLOCK_STATUS_VALUE: u64 = 0xf;
pub(crate) const VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE: u64 = 0;
pub(crate) const VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS: u64 = 0x4000;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS: u64 = 0x4000_1000;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS: u64 = 0x4000_2000;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS: u64 = 0x4000_3000;
pub(crate) const VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE: u64 = 0;
pub(crate) const VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE: u64 = 0;
pub(crate) const VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE: u64 = 0x1;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS: u64 = 0x4000_0080;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS: u64 = 0x4000_0400;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS: u64 = 0x4000_0700;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS: u64 = 0x4000_0800;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS: u64 = 0x4000_0900;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS: u64 = 0x4000_0c00;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS: u64 = 0x4000_0d00;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS: u64 = 0x4000_0e00;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR: u64 = 7;
pub(crate) const VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES: u32 = 512;
pub(crate) const VIRTIO_BLOCK_SECTOR_BYTES: u64 = 512;
pub(crate) const VIRTQ_DESC_SIZE: u64 = 16;
pub(crate) const VIRTQ_DESC_F_NEXT: u16 = 0x1;
pub(crate) const VIRTQ_DESC_F_WRITE: u16 = 0x2;
pub(crate) const VIRTIO_BLK_T_IN: u32 = 0;
pub(crate) const VIRTIO_BLK_T_OUT: u32 = 1;
pub(crate) const VIRTIO_BLK_T_FLUSH: u32 = 4;
pub(crate) const VIRTIO_BLK_F_RO: u64 = 1 << 5;
pub(crate) const VIRTIO_BLK_S_OK: u8 = 0;
pub(crate) const VIRTIO_BLK_S_IOERR: u8 = 1;
pub(crate) const VIRTIO_BLOCK_REQUEST_HEADER_BYTES: u32 = 16;
pub(crate) const VIRTIO_BLOCK_STATUS_BYTES: u32 = 1;
pub(crate) const VIRTIO_BLOCK_MAX_SYNTHETIC_IO_BYTES: u32 = 4096;
pub(crate) const BOOT_MMIO_DEVICE_MODELS: &str =
    "PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton";
pub(crate) const BLOCK_QUEUE_MMIO_DEVICE_MODELS: &str = "PL011 UART skeleton; PL031 RTC skeleton; VirtIO-MMIO block identity skeleton; VirtIO-MMIO block queue/config/address/notify skeleton";
pub(crate) const WINDOWS_ARM_FIRMWARE_MMIO_DEVICE_MODELS: &str = "PL011 UART skeleton; PL031 RTC skeleton; GICv3 distributor MMIO skeleton; GICv3 redistributor MMIO skeleton; VirtIO-MMIO installer ISO block skeleton; VirtIO-MMIO target disk block skeleton";

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pl011UartDevice {
    pub(crate) base_ipa: u64,
    pub(crate) flag_value: u64,
}

impl Pl011UartDevice {
    pub(crate) fn new(base_ipa: u64, flag_value: u64) -> Self {
        Self {
            base_ipa,
            flag_value,
        }
    }

    pub(crate) fn data_ipa(&self) -> u64 {
        self.base_ipa + PL011_DR_OFFSET
    }

    pub(crate) fn flags_ipa(&self) -> u64 {
        self.base_ipa + PL011_FR_OFFSET
    }
}

impl MmioDevice for Pl011UartDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: PL011_REGISTER_WINDOW_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        match (access.kind, access.ipa, access.value) {
            (MmioAccessKind::Write, ipa, Some(value)) if ipa == self.data_ipa() => {
                let mask = if access.width >= 8 {
                    u64::MAX
                } else {
                    (1_u64 << (u64::from(access.width) * 8)) - 1
                };
                let value = value & mask;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Read, ipa, None) if ipa == self.flags_ipa() => {
                MmioAction::ReadValue(self.flag_value)
            }
            _ => MmioAction::Unhandled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Pl031RtcDevice {
    pub(crate) base_ipa: u64,
    pub(crate) data_value: u64,
}

impl Pl031RtcDevice {
    pub(crate) fn new(base_ipa: u64, data_value: u64) -> Self {
        Self {
            base_ipa,
            data_value,
        }
    }

    pub(crate) fn data_ipa(&self) -> u64 {
        self.base_ipa + PL031_DR_OFFSET
    }
}

impl MmioDevice for Pl031RtcDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: PL031_REGISTER_WINDOW_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        match (access.kind, access.ipa, access.value) {
            (MmioAccessKind::Read, ipa, None) if ipa == self.data_ipa() => {
                MmioAction::ReadValue(self.data_value)
            }
            _ => MmioAction::Unhandled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GicV3DistributorDevice {
    pub(crate) base_ipa: u64,
    pub(crate) ctlr: u32,
    pub(crate) statusr: u32,
    pub(crate) group: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) group_modifier: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) enabled: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) pending: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) active: [u32; GICV3_INTERRUPT_REGISTER_COUNT],
    pub(crate) priority: [u32; GICV3_PRIORITY_REGISTER_COUNT],
    pub(crate) config: [u32; GICV3_CONFIG_REGISTER_COUNT],
    pub(crate) route: [u64; GICV3_SUPPORTED_INTERRUPT_COUNT],
}

impl GicV3DistributorDevice {
    pub(crate) fn new(base_ipa: u64) -> Self {
        Self {
            base_ipa,
            ctlr: 0,
            statusr: 0,
            group: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            group_modifier: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            enabled: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            pending: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            active: [0; GICV3_INTERRUPT_REGISTER_COUNT],
            priority: [GICV3_DEFAULT_PRIORITY_WORD; GICV3_PRIORITY_REGISTER_COUNT],
            config: [0; GICV3_CONFIG_REGISTER_COUNT],
            route: [0; GICV3_SUPPORTED_INTERRUPT_COUNT],
        }
    }

    pub(crate) fn reg_index(offset: u64, base: u64, count: usize) -> Option<usize> {
        let end = base.checked_add((count as u64).checked_mul(4)?)?;
        if offset < base || offset >= end || (offset - base) % 4 != 0 {
            return None;
        }
        usize::try_from((offset - base) / 4).ok()
    }

    pub(crate) fn irouter_interrupt_id(offset: u64) -> Option<usize> {
        if !(GICD_IROUTER_BASE_OFFSET..GICD_IROUTER_BASE_OFFSET + 0x2000).contains(&offset) {
            return None;
        }
        let relative = offset - GICD_IROUTER_BASE_OFFSET;
        let interrupt_id = usize::try_from(relative / 8).ok()?;
        (interrupt_id < GICV3_SUPPORTED_INTERRUPT_COUNT).then_some(interrupt_id)
    }

    pub(crate) fn read_u64_register(offset: u64, base: u64, value: u64, width: u8) -> Option<u64> {
        match offset {
            current if current == base => Some(if width >= 8 {
                value
            } else {
                value & 0xffff_ffff
            }),
            current if current == base + 4 => Some(value >> 32),
            _ => None,
        }
    }

    pub(crate) fn write_u64_register(
        current: u64,
        offset: u64,
        base: u64,
        value: u64,
        width: u8,
    ) -> Option<u64> {
        let value = mask_mmio_value(value, width);
        match offset {
            current_offset if current_offset == base => Some(if width >= 8 {
                value
            } else {
                (current & 0xffff_ffff_0000_0000) | (value & 0xffff_ffff)
            }),
            current_offset if current_offset == base + 4 => {
                Some((current & 0x0000_0000_ffff_ffff) | ((value & 0xffff_ffff) << 32))
            }
            _ => None,
        }
    }

    pub(crate) fn read_indexed_register(
        offset: u64,
        base: u64,
        registers: &[u32],
    ) -> Option<MmioAction> {
        Self::reg_index(offset, base, registers.len())
            .map(|index| MmioAction::ReadValue(u64::from(registers[index])))
    }

    pub(crate) fn byte_register_access_offset(
        offset: u64,
        base: u64,
        registers: &[u32],
        width: u8,
    ) -> Option<usize> {
        let access_bytes = usize::from(width);
        if access_bytes == 0 || access_bytes > 8 {
            return None;
        }
        let end = base.checked_add((registers.len() as u64).checked_mul(4)?)?;
        let access_end = offset.checked_add(u64::from(width))?;
        if offset < base || access_end > end {
            return None;
        }
        usize::try_from(offset - base).ok()
    }

    pub(crate) fn read_byte_indexed_register(
        offset: u64,
        base: u64,
        registers: &[u32],
        width: u8,
    ) -> Option<MmioAction> {
        let byte_offset = Self::byte_register_access_offset(offset, base, registers, width)?;
        let mut value = 0_u64;
        for byte_index in 0..usize::from(width) {
            let absolute_byte = byte_offset + byte_index;
            let register = registers[absolute_byte / 4];
            let register_shift = (absolute_byte % 4) * 8;
            let byte = (register >> register_shift) & 0xff;
            value |= u64::from(byte) << (byte_index * 8);
        }
        Some(MmioAction::ReadValue(value))
    }

    pub(crate) fn write_byte_indexed_register(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let byte_offset = Self::byte_register_access_offset(offset, base, registers, width)?;
        let value = mask_mmio_value(value, width);
        for byte_index in 0..usize::from(width) {
            let absolute_byte = byte_offset + byte_index;
            let register = &mut registers[absolute_byte / 4];
            let register_shift = (absolute_byte % 4) * 8;
            let mask = 0xff_u32 << register_shift;
            let byte = ((value >> (byte_index * 8)) as u32 & 0xff) << register_shift;
            *register = (*register & !mask) | byte;
        }
        Some(MmioAction::WriteAccepted {
            value,
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn write_indexed_register(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let index = Self::reg_index(offset, base, registers.len())?;
        let value = mask_mmio_value(value, width) as u32;
        registers[index] = value;
        Some(MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn set_indexed_bits(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let index = Self::reg_index(offset, base, registers.len())?;
        let value = mask_mmio_value(value, width) as u32;
        registers[index] |= value;
        Some(MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn clear_indexed_bits(
        offset: u64,
        base: u64,
        registers: &mut [u32],
        value: u64,
        width: u8,
    ) -> Option<MmioAction> {
        let index = Self::reg_index(offset, base, registers.len())?;
        let value = mask_mmio_value(value, width) as u32;
        registers[index] &= !value;
        Some(MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        })
    }

    pub(crate) fn interrupt_bit(interrupt_id: usize) -> Option<(usize, u32)> {
        if interrupt_id >= GICV3_SUPPORTED_INTERRUPT_COUNT {
            return None;
        }
        Some((interrupt_id / 32, 1_u32 << (interrupt_id % 32)))
    }

    pub(crate) fn spi_interrupt_id(spi: u32) -> Option<usize> {
        let interrupt_id = 32_usize.checked_add(usize::try_from(spi).ok()?)?;
        (interrupt_id < GICV3_SUPPORTED_INTERRUPT_COUNT).then_some(interrupt_id)
    }

    pub(crate) fn set_spi_pending(&mut self, spi: u32, pending: bool) -> Option<()> {
        let interrupt_id = Self::spi_interrupt_id(spi)?;
        let (register, bit) = Self::interrupt_bit(interrupt_id)?;
        if pending {
            self.pending[register] |= bit;
        } else {
            self.pending[register] &= !bit;
        }
        Some(())
    }

    pub(crate) fn spi_irq_line_assertable(&self, spi: u32) -> bool {
        let Some(interrupt_id) = Self::spi_interrupt_id(spi) else {
            return false;
        };
        let Some((register, bit)) = Self::interrupt_bit(interrupt_id) else {
            return false;
        };
        (self.ctlr & GICD_CTLR_ENABLE_GRP1NS) != 0
            && (self.group[register] & bit) != 0
            && (self.enabled[register] & bit) != 0
            && (self.pending[register] & bit) != 0
            && (self.active[register] & bit) == 0
    }

    pub(crate) fn interrupt_priority(&self, interrupt_id: usize) -> Option<u8> {
        if interrupt_id >= GICV3_SUPPORTED_INTERRUPT_COUNT {
            return None;
        }
        let register = interrupt_id / 4;
        let shift = (interrupt_id % 4) * 8;
        Some(((self.priority[register] >> shift) & 0xff) as u8)
    }

    pub(crate) fn pending_interrupt_for_cpu(
        &self,
        priority_mask: u8,
    ) -> Option<GicV3PendingInterrupt> {
        if self.ctlr & GICD_CTLR_ENABLE_GRP1NS == 0 {
            return None;
        }
        (32..GICV3_SUPPORTED_INTERRUPT_COUNT)
            .filter_map(|interrupt_id| {
                let (register, bit) = Self::interrupt_bit(interrupt_id)?;
                let group1 = (self.group[register] & bit) != 0;
                let enabled = (self.enabled[register] & bit) != 0;
                let pending = (self.pending[register] & bit) != 0;
                let active = (self.active[register] & bit) != 0;
                let priority = self.interrupt_priority(interrupt_id)?;
                (group1 && enabled && pending && !active && priority < priority_mask).then_some(
                    GicV3PendingInterrupt {
                        interrupt_id: interrupt_id as u32,
                        priority,
                    },
                )
            })
            .min_by_key(|interrupt| (interrupt.priority, interrupt.interrupt_id))
    }

    #[cfg(test)]
    pub(crate) fn pending_interrupt_id_for_cpu(&self, priority_mask: u8) -> Option<u32> {
        self.pending_interrupt_for_cpu(priority_mask)
            .map(|interrupt| interrupt.interrupt_id)
    }

    pub(crate) fn acknowledge_interrupt_id(&mut self, interrupt_id: u32) -> bool {
        let Ok(interrupt_id) = usize::try_from(interrupt_id) else {
            return false;
        };
        let Some((register, bit)) = Self::interrupt_bit(interrupt_id) else {
            return false;
        };
        let was_pending = (self.pending[register] & bit) != 0;
        if was_pending {
            self.pending[register] &= !bit;
            self.active[register] |= bit;
        }
        was_pending
    }

    #[cfg(test)]
    pub(crate) fn acknowledge_pending_interrupt(&mut self, priority_mask: u8) -> u32 {
        let Some(interrupt) = self.pending_interrupt_for_cpu(priority_mask) else {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        };
        if !self.acknowledge_interrupt_id(interrupt.interrupt_id) {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        }
        interrupt.interrupt_id
    }

    pub(crate) fn end_interrupt(&mut self, interrupt_id: u32) -> bool {
        let Ok(interrupt_id) = usize::try_from(interrupt_id) else {
            return false;
        };
        let Some((register, bit)) = Self::interrupt_bit(interrupt_id) else {
            return false;
        };
        let was_active = (self.active[register] & bit) != 0;
        self.active[register] &= !bit;
        was_active
    }
}

impl MmioDevice for GicV3DistributorDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: WINDOWS_ARM_GIC_DISTRIBUTOR_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        let offset = access.ipa.saturating_sub(self.base_ipa);
        match (access.kind, offset, access.value) {
            (MmioAccessKind::Read, GICD_CTLR_OFFSET, None) => {
                MmioAction::ReadValue(u64::from(self.ctlr))
            }
            (MmioAccessKind::Read, GICD_TYPER_OFFSET, None) => {
                MmioAction::ReadValue(GICD_TYPER_VALUE)
            }
            (MmioAccessKind::Read, GICD_IIDR_OFFSET, None) => {
                MmioAction::ReadValue(GICV3_IIDR_VALUE)
            }
            (MmioAccessKind::Read, GICD_STATUSR_OFFSET, None) => {
                MmioAction::ReadValue(u64::from(self.statusr))
            }
            (MmioAccessKind::Read, current, None) => {
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_IGROUPR_BASE_OFFSET, &self.group)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ISENABLER_BASE_OFFSET, &self.enabled)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICENABLER_BASE_OFFSET, &self.enabled)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ISPENDR_BASE_OFFSET, &self.pending)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICPENDR_BASE_OFFSET, &self.pending)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ISACTIVER_BASE_OFFSET, &self.active)
                {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICACTIVER_BASE_OFFSET, &self.active)
                {
                    return action;
                }
                if let Some(action) = Self::read_byte_indexed_register(
                    current,
                    GICD_IPRIORITYR_BASE_OFFSET,
                    &self.priority,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) =
                    Self::read_indexed_register(current, GICD_ICFGR_BASE_OFFSET, &self.config)
                {
                    return action;
                }
                if let Some(action) = Self::read_indexed_register(
                    current,
                    GICD_IGRPMODR_BASE_OFFSET,
                    &self.group_modifier,
                ) {
                    return action;
                }
                if let Some(interrupt_id) = Self::irouter_interrupt_id(current) {
                    let base = GICD_IROUTER_BASE_OFFSET + (interrupt_id as u64 * 8);
                    if let Some(value) = Self::read_u64_register(
                        current,
                        base,
                        self.route[interrupt_id],
                        access.width,
                    ) {
                        return MmioAction::ReadValue(value);
                    }
                }
                MmioAction::Unhandled
            }
            (MmioAccessKind::Write, GICD_CTLR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width) as u32;
                self.ctlr = value;
                MmioAction::WriteAccepted {
                    value: u64::from(value),
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, GICD_STATUSR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width) as u32;
                self.statusr &= !value;
                MmioAction::WriteAccepted {
                    value: u64::from(value),
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, current, Some(value)) => {
                if let Some(action) = Self::write_indexed_register(
                    current,
                    GICD_IGROUPR_BASE_OFFSET,
                    &mut self.group,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::set_indexed_bits(
                    current,
                    GICD_ISENABLER_BASE_OFFSET,
                    &mut self.enabled,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::clear_indexed_bits(
                    current,
                    GICD_ICENABLER_BASE_OFFSET,
                    &mut self.enabled,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::set_indexed_bits(
                    current,
                    GICD_ISPENDR_BASE_OFFSET,
                    &mut self.pending,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::clear_indexed_bits(
                    current,
                    GICD_ICPENDR_BASE_OFFSET,
                    &mut self.pending,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::set_indexed_bits(
                    current,
                    GICD_ISACTIVER_BASE_OFFSET,
                    &mut self.active,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::clear_indexed_bits(
                    current,
                    GICD_ICACTIVER_BASE_OFFSET,
                    &mut self.active,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::write_byte_indexed_register(
                    current,
                    GICD_IPRIORITYR_BASE_OFFSET,
                    &mut self.priority,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::write_indexed_register(
                    current,
                    GICD_ICFGR_BASE_OFFSET,
                    &mut self.config,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(action) = Self::write_indexed_register(
                    current,
                    GICD_IGRPMODR_BASE_OFFSET,
                    &mut self.group_modifier,
                    value,
                    access.width,
                ) {
                    return action;
                }
                if let Some(interrupt_id) = Self::irouter_interrupt_id(current) {
                    let base = GICD_IROUTER_BASE_OFFSET + (interrupt_id as u64 * 8);
                    if let Some(routing) = Self::write_u64_register(
                        self.route[interrupt_id],
                        current,
                        base,
                        value,
                        access.width,
                    ) {
                        self.route[interrupt_id] = routing;
                        let value = mask_mmio_value(value, access.width);
                        return MmioAction::WriteAccepted {
                            value,
                            byte: (value & 0xff) as u8,
                        };
                    }
                }
                MmioAction::Unhandled
            }
            _ => MmioAction::Unhandled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GicV3RedistributorDevice {
    pub(crate) base_ipa: u64,
    pub(crate) ctlr: u32,
    pub(crate) waker: u32,
    pub(crate) propbaser: u64,
    pub(crate) pendbaser: u64,
    pub(crate) group0: u32,
    pub(crate) group_modifier0: u32,
    pub(crate) enabled0: u32,
    pub(crate) pending0: u32,
    pub(crate) active0: u32,
    pub(crate) priority: [u32; 8],
    pub(crate) config0: u32,
}

impl GicV3RedistributorDevice {
    pub(crate) fn new(base_ipa: u64) -> Self {
        Self {
            base_ipa,
            ctlr: 0,
            waker: 0,
            propbaser: 0,
            pendbaser: 0,
            group0: 0,
            group_modifier0: 0,
            enabled0: 0,
            pending0: 0,
            active0: 0,
            priority: [GICV3_DEFAULT_PRIORITY_WORD; 8],
            config0: 0,
        }
    }

    pub(crate) fn write_waker(&mut self, value: u64, width: u8) -> MmioAction {
        let value = mask_mmio_value(value, width) as u32;
        if value & GICR_WAKER_PROCESSOR_SLEEP as u32 != 0 {
            self.waker = (GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP) as u32;
        } else {
            self.waker = 0;
        }
        MmioAction::WriteAccepted {
            value: u64::from(value),
            byte: (value & 0xff) as u8,
        }
    }

    pub(crate) fn fdt_ppi_interrupt_id(ppi: u32) -> Option<u32> {
        let interrupt_id = 16_u32.checked_add(ppi)?;
        (interrupt_id < 32).then_some(interrupt_id)
    }

    pub(crate) fn interrupt_priority(&self, interrupt_id: u32) -> Option<u8> {
        if interrupt_id >= 32 {
            return None;
        }
        let register = usize::try_from(interrupt_id / 4).ok()?;
        let shift = (interrupt_id % 4) * 8;
        Some(((self.priority[register] >> shift) & 0xff) as u8)
    }

    pub(crate) fn set_fdt_ppi_pending(&mut self, ppi: u32, pending: bool) -> bool {
        let Some(interrupt_id) = Self::fdt_ppi_interrupt_id(ppi) else {
            return false;
        };
        let bit = 1_u32 << interrupt_id;
        if pending {
            self.pending0 |= bit;
        } else {
            self.pending0 &= !bit;
        }
        true
    }

    pub(crate) fn pending_interrupt_for_cpu(
        &self,
        priority_mask: u8,
    ) -> Option<GicV3PendingInterrupt> {
        if self.waker & GICR_WAKER_PROCESSOR_SLEEP as u32 != 0 {
            return None;
        }
        (16_u32..32)
            .filter_map(|interrupt_id| {
                let bit = 1_u32 << interrupt_id;
                let group1 = (self.group0 & bit) != 0;
                let enabled = (self.enabled0 & bit) != 0;
                let pending = (self.pending0 & bit) != 0;
                let active = (self.active0 & bit) != 0;
                let priority = self.interrupt_priority(interrupt_id)?;
                (group1 && enabled && pending && !active && priority < priority_mask).then_some(
                    GicV3PendingInterrupt {
                        interrupt_id,
                        priority,
                    },
                )
            })
            .min_by_key(|interrupt| (interrupt.priority, interrupt.interrupt_id))
    }

    #[cfg(test)]
    pub(crate) fn pending_interrupt_id_for_cpu(&self, priority_mask: u8) -> Option<u32> {
        self.pending_interrupt_for_cpu(priority_mask)
            .map(|interrupt| interrupt.interrupt_id)
    }

    pub(crate) fn acknowledge_interrupt_id(&mut self, interrupt_id: u32) -> bool {
        if interrupt_id >= 32 {
            return false;
        }
        let bit = 1_u32 << interrupt_id;
        let was_pending = (self.pending0 & bit) != 0;
        if was_pending {
            self.pending0 &= !bit;
            self.active0 |= bit;
        }
        was_pending
    }

    #[cfg(test)]
    pub(crate) fn acknowledge_pending_interrupt(&mut self, priority_mask: u8) -> u32 {
        let Some(interrupt) = self.pending_interrupt_for_cpu(priority_mask) else {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        };
        if !self.acknowledge_interrupt_id(interrupt.interrupt_id) {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        }
        interrupt.interrupt_id
    }

    pub(crate) fn end_interrupt(&mut self, interrupt_id: u32) -> bool {
        if interrupt_id >= 32 {
            return false;
        }
        let bit = 1_u32 << interrupt_id;
        let was_active = (self.active0 & bit) != 0;
        self.active0 &= !bit;
        was_active
    }
}

impl MmioDevice for GicV3RedistributorDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: WINDOWS_ARM_GIC_REDISTRIBUTOR_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        let offset = access.ipa.saturating_sub(self.base_ipa);
        match (access.kind, offset, access.value) {
            (MmioAccessKind::Read, GICR_CTLR_OFFSET, None) => {
                MmioAction::ReadValue(u64::from(self.ctlr))
            }
            (MmioAccessKind::Read, GICR_IIDR_OFFSET, None) => {
                MmioAction::ReadValue(GICV3_IIDR_VALUE)
            }
            (MmioAccessKind::Read, current, None) => {
                if let Some(value) = GicV3DistributorDevice::read_u64_register(
                    current,
                    GICR_TYPER_OFFSET,
                    GICR_TYPER_VALUE,
                    access.width,
                ) {
                    return MmioAction::ReadValue(value);
                }
                match current {
                    GICR_STATUSR_OFFSET => MmioAction::ReadValue(0),
                    GICR_WAKER_OFFSET => MmioAction::ReadValue(u64::from(self.waker)),
                    GICR_SGI_IGROUPR0_OFFSET => MmioAction::ReadValue(u64::from(self.group0)),
                    GICR_SGI_ISENABLER0_OFFSET | GICR_SGI_ICENABLER0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.enabled0))
                    }
                    GICR_SGI_ISPENDR0_OFFSET | GICR_SGI_ICPENDR0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.pending0))
                    }
                    GICR_SGI_ISACTIVER0_OFFSET | GICR_SGI_ICACTIVER0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.active0))
                    }
                    GICR_SGI_ICFGR0_OFFSET => MmioAction::ReadValue(u64::from(self.config0)),
                    GICR_SGI_IGRPMODR0_OFFSET => {
                        MmioAction::ReadValue(u64::from(self.group_modifier0))
                    }
                    _ => {
                        if let Some(value) = GicV3DistributorDevice::read_u64_register(
                            current,
                            GICR_PROPBASER_OFFSET,
                            self.propbaser,
                            access.width,
                        ) {
                            return MmioAction::ReadValue(value);
                        }
                        if let Some(value) = GicV3DistributorDevice::read_u64_register(
                            current,
                            GICR_PENDBASER_OFFSET,
                            self.pendbaser,
                            access.width,
                        ) {
                            return MmioAction::ReadValue(value);
                        }
                        if let Some(action) = GicV3DistributorDevice::read_byte_indexed_register(
                            current,
                            GICR_SGI_IPRIORITYR_BASE_OFFSET,
                            &self.priority,
                            access.width,
                        ) {
                            return action;
                        }
                        MmioAction::Unhandled
                    }
                }
            }
            (MmioAccessKind::Write, GICR_CTLR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width) as u32;
                self.ctlr = value;
                MmioAction::WriteAccepted {
                    value: u64::from(value),
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, GICR_WAKER_OFFSET, Some(value)) => {
                self.write_waker(value, access.width)
            }
            (MmioAccessKind::Write, GICR_STATUSR_OFFSET, Some(value)) => {
                let value = mask_mmio_value(value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, current, Some(value)) => match current {
                GICR_SGI_IGROUPR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.group0 = value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ISENABLER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.enabled0 |= value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICENABLER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.enabled0 &= !value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ISPENDR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.pending0 |= value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICPENDR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.pending0 &= !value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ISACTIVER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.active0 |= value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICACTIVER0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.active0 &= !value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_ICFGR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.config0 = value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                GICR_SGI_IGRPMODR0_OFFSET => {
                    let value = mask_mmio_value(value, access.width) as u32;
                    self.group_modifier0 = value;
                    MmioAction::WriteAccepted {
                        value: u64::from(value),
                        byte: (value & 0xff) as u8,
                    }
                }
                _ => {
                    if let Some(propbaser) = GicV3DistributorDevice::write_u64_register(
                        self.propbaser,
                        current,
                        GICR_PROPBASER_OFFSET,
                        value,
                        access.width,
                    ) {
                        self.propbaser = propbaser;
                        let value = mask_mmio_value(value, access.width);
                        return MmioAction::WriteAccepted {
                            value,
                            byte: (value & 0xff) as u8,
                        };
                    }
                    if let Some(pendbaser) = GicV3DistributorDevice::write_u64_register(
                        self.pendbaser,
                        current,
                        GICR_PENDBASER_OFFSET,
                        value,
                        access.width,
                    ) {
                        self.pendbaser = pendbaser;
                        let value = mask_mmio_value(value, access.width);
                        return MmioAction::WriteAccepted {
                            value,
                            byte: (value & 0xff) as u8,
                        };
                    }
                    if let Some(action) = GicV3DistributorDevice::write_byte_indexed_register(
                        current,
                        GICR_SGI_IPRIORITYR_BASE_OFFSET,
                        &mut self.priority,
                        value,
                        access.width,
                    ) {
                        return action;
                    }
                    MmioAction::Unhandled
                }
            },
            _ => MmioAction::Unhandled,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtioMmioBlockDevice {
    pub(crate) base_ipa: u64,
    pub(crate) device_features: u64,
    pub(crate) driver_features: u64,
    pub(crate) queue_select: u64,
    pub(crate) queue_num_max: u64,
    pub(crate) queue_num: u64,
    pub(crate) queue_ready: u64,
    pub(crate) queue_notify: u64,
    pub(crate) queue_desc: u64,
    pub(crate) queue_driver: u64,
    pub(crate) queue_device: u64,
    pub(crate) interrupt_status: u64,
    pub(crate) interrupt_ack: u64,
    pub(crate) last_avail_idx: u16,
    pub(crate) completed_requests: u64,
    pub(crate) status: u64,
    pub(crate) config_generation: u64,
    pub(crate) capacity_sectors: u64,
}

impl VirtioMmioBlockDevice {
    pub(crate) fn new(base_ipa: u64) -> Self {
        Self::new_with_features_and_capacity(
            base_ipa,
            VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE,
            VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS,
        )
    }

    pub(crate) fn from_metadata(device: &WindowsArmVirtioBlockDeviceMetadata) -> Self {
        Self::new_with_features_and_capacity(
            device.base_ipa,
            device.device_features,
            device.capacity_sectors,
        )
    }

    pub(crate) fn new_with_features_and_capacity(
        base_ipa: u64,
        device_features: u64,
        capacity_sectors: u64,
    ) -> Self {
        Self {
            base_ipa,
            device_features,
            driver_features: 0,
            queue_select: 0,
            queue_num_max: VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
            queue_num: 0,
            queue_ready: 0,
            queue_notify: 0,
            queue_desc: 0,
            queue_driver: 0,
            queue_device: 0,
            interrupt_status: VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
            interrupt_ack: 0,
            last_avail_idx: 0,
            completed_requests: 0,
            status: 0,
            config_generation: VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE,
            capacity_sectors,
        }
    }

    pub(crate) fn mask_value(value: u64, width: u8) -> u64 {
        if width >= 8 {
            value
        } else {
            value & ((1_u64 << (u64::from(width) * 8)) - 1)
        }
    }

    pub(crate) fn replace_low_32(current: u64, value: u64, width: u8) -> u64 {
        let value = Self::mask_value(value, width) & 0xffff_ffff;
        (current & 0xffff_ffff_0000_0000) | value
    }

    pub(crate) fn replace_high_32(current: u64, value: u64, width: u8) -> u64 {
        let value = Self::mask_value(value, width) & 0xffff_ffff;
        (current & 0x0000_0000_ffff_ffff) | (value << 32)
    }

    pub(crate) fn reset_driver_state(&mut self) {
        self.driver_features = 0;
        self.queue_select = 0;
        self.queue_num = 0;
        self.queue_ready = 0;
        self.queue_notify = 0;
        self.queue_desc = 0;
        self.queue_driver = 0;
        self.queue_device = 0;
        self.interrupt_status = VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE;
        self.interrupt_ack = 0;
        self.last_avail_idx = 0;
        self.completed_requests = 0;
        self.status = 0;
    }

    pub(crate) fn complete_next_available_block_request(
        &mut self,
        memory: &mut VirtioGuestMemory<'_>,
    ) -> Result<VirtioBlockRequestCompletion, VirtioBlockRequestError> {
        let mut backend = SyntheticBlockStorageBackend;
        self.complete_next_available_block_request_from_backend(memory, &mut backend)
    }

    pub(crate) fn complete_next_available_block_request_from_backend(
        &mut self,
        memory: &mut VirtioGuestMemory<'_>,
        backend: &mut impl VirtioBlockStorageBackend,
    ) -> Result<VirtioBlockRequestCompletion, VirtioBlockRequestError> {
        if self.queue_ready != VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE {
            return Err(VirtioBlockRequestError::QueueNotReady);
        }

        let queue_size = u16::try_from(self.queue_num)
            .ok()
            .filter(|value| *value > 0)
            .ok_or(VirtioBlockRequestError::InvalidQueueSize(self.queue_num))?;
        if u64::from(queue_size) > self.queue_num_max {
            return Err(VirtioBlockRequestError::InvalidQueueSize(self.queue_num));
        }

        let avail_idx = memory.read_u16(self.queue_driver + 2)?;
        if avail_idx == self.last_avail_idx {
            return Err(VirtioBlockRequestError::NoAvailableRequest);
        }

        let avail_slot = u64::from(self.last_avail_idx % queue_size);
        let descriptor_index = memory.read_u16(self.queue_driver + 4 + (avail_slot * 2))?;
        let header_desc =
            VirtqDescriptor::read(memory, self.queue_desc, descriptor_index, queue_size)?;
        if header_desc.len < VIRTIO_BLOCK_REQUEST_HEADER_BYTES {
            return Err(VirtioBlockRequestError::DescriptorTooSmall {
                role: "request header",
                len: header_desc.len,
            });
        }
        if header_desc.flags & VIRTQ_DESC_F_WRITE != 0 {
            return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                role: "request header",
                flags: header_desc.flags,
            });
        }
        if header_desc.flags & VIRTQ_DESC_F_NEXT == 0 {
            return Err(VirtioBlockRequestError::MissingNextDescriptor(
                "request header",
            ));
        }

        let request_type = memory.read_u32(header_desc.addr)?;
        if !matches!(
            request_type,
            VIRTIO_BLK_T_IN | VIRTIO_BLK_T_OUT | VIRTIO_BLK_T_FLUSH
        ) {
            return Err(VirtioBlockRequestError::UnsupportedRequestType(
                request_type,
            ));
        }
        let sector = memory.read_u64(header_desc.addr + 8)?;

        let (status_desc, data_bytes, used_len, status) = match request_type {
            VIRTIO_BLK_T_FLUSH => {
                let status_desc =
                    VirtqDescriptor::read(memory, self.queue_desc, header_desc.next, queue_size)?;
                if status_desc.len < VIRTIO_BLOCK_STATUS_BYTES {
                    return Err(VirtioBlockRequestError::DescriptorTooSmall {
                        role: "status",
                        len: status_desc.len,
                    });
                }
                if status_desc.flags & VIRTQ_DESC_F_WRITE == 0 {
                    return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                        role: "status",
                        flags: status_desc.flags,
                    });
                }
                let status = match backend.flush() {
                    Ok(()) => VIRTIO_BLK_S_OK,
                    Err(VirtioBlockRequestError::StorageWriteRejected { .. }) => VIRTIO_BLK_S_IOERR,
                    Err(error) => return Err(error),
                };
                (status_desc, 0, VIRTIO_BLOCK_STATUS_BYTES, status)
            }
            VIRTIO_BLK_T_IN | VIRTIO_BLK_T_OUT => {
                let data_desc =
                    VirtqDescriptor::read(memory, self.queue_desc, header_desc.next, queue_size)?;
                if data_desc.len == 0 || data_desc.len > VIRTIO_BLOCK_MAX_SYNTHETIC_IO_BYTES {
                    return Err(VirtioBlockRequestError::InvalidDataLength(data_desc.len));
                }
                match request_type {
                    VIRTIO_BLK_T_IN => {
                        if data_desc.flags & VIRTQ_DESC_F_WRITE == 0
                            || data_desc.flags & VIRTQ_DESC_F_NEXT == 0
                        {
                            return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                                role: "data",
                                flags: data_desc.flags,
                            });
                        }
                    }
                    VIRTIO_BLK_T_OUT => {
                        if data_desc.flags & VIRTQ_DESC_F_WRITE != 0
                            || data_desc.flags & VIRTQ_DESC_F_NEXT == 0
                        {
                            return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                                role: "data",
                                flags: data_desc.flags,
                            });
                        }
                    }
                    _ => unreachable!("request_type checked above"),
                }

                let status_desc =
                    VirtqDescriptor::read(memory, self.queue_desc, data_desc.next, queue_size)?;
                if status_desc.len < VIRTIO_BLOCK_STATUS_BYTES {
                    return Err(VirtioBlockRequestError::DescriptorTooSmall {
                        role: "status",
                        len: status_desc.len,
                    });
                }
                if status_desc.flags & VIRTQ_DESC_F_WRITE == 0 {
                    return Err(VirtioBlockRequestError::UnexpectedDescriptorFlags {
                        role: "status",
                        flags: status_desc.flags,
                    });
                }

                let byte_offset = sector
                    .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
                    .ok_or(VirtioBlockRequestError::StorageOffsetOverflow { sector })?;
                let status = match request_type {
                    VIRTIO_BLK_T_IN => {
                        let mut data = vec![0_u8; data_desc.len as usize];
                        backend.read_exact_at(byte_offset, &mut data)?;
                        memory.write_bytes(data_desc.addr, &data)?;
                        VIRTIO_BLK_S_OK
                    }
                    VIRTIO_BLK_T_OUT => {
                        let data = memory.read_slice(data_desc.addr, data_desc.len as usize)?;
                        match backend.write_exact_at(byte_offset, data) {
                            Ok(()) => VIRTIO_BLK_S_OK,
                            Err(VirtioBlockRequestError::StorageWriteRejected { .. }) => {
                                VIRTIO_BLK_S_IOERR
                            }
                            Err(error) => return Err(error),
                        }
                    }
                    _ => unreachable!("request_type checked above"),
                };
                let used_len = match request_type {
                    VIRTIO_BLK_T_IN => data_desc.len + VIRTIO_BLOCK_STATUS_BYTES,
                    VIRTIO_BLK_T_OUT => VIRTIO_BLOCK_STATUS_BYTES,
                    _ => unreachable!("request_type checked above"),
                };
                (status_desc, data_desc.len, used_len, status)
            }
            _ => unreachable!("request_type checked above"),
        };

        memory.write_u8(status_desc.addr, status)?;

        let used_idx = memory.read_u16(self.queue_device + 2)?;
        let used_slot = u64::from(used_idx % queue_size);
        let used_elem = self.queue_device + 4 + (used_slot * 8);
        memory.write_u32(used_elem, u32::from(descriptor_index))?;
        memory.write_u32(used_elem + 4, used_len)?;
        memory.write_u16(self.queue_device + 2, used_idx.wrapping_add(1))?;

        self.last_avail_idx = self.last_avail_idx.wrapping_add(1);
        self.completed_requests = self.completed_requests.saturating_add(1);
        self.interrupt_status |= VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE;

        Ok(VirtioBlockRequestCompletion {
            descriptor_index,
            request_type,
            sector,
            data_bytes,
            status,
            used_index: used_idx.wrapping_add(1),
            interrupt_status: self.interrupt_status,
        })
    }
}

#[derive(Debug)]
pub(crate) struct VirtioGuestMemory<'a> {
    pub(crate) base_ipa: u64,
    pub(crate) bytes: &'a mut [u8],
}

impl<'a> VirtioGuestMemory<'a> {
    pub(crate) fn new(base_ipa: u64, bytes: &'a mut [u8]) -> Self {
        Self { base_ipa, bytes }
    }

    pub(crate) fn range(
        &self,
        ipa: u64,
        len: usize,
    ) -> Result<std::ops::Range<usize>, VirtioBlockRequestError> {
        let offset = ipa
            .checked_sub(self.base_ipa)
            .ok_or(VirtioBlockRequestError::MemoryOutOfRange { ipa, len })?;
        let offset = usize::try_from(offset)
            .map_err(|_| VirtioBlockRequestError::MemoryOutOfRange { ipa, len })?;
        let end = offset
            .checked_add(len)
            .ok_or(VirtioBlockRequestError::MemoryOutOfRange { ipa, len })?;
        if end > self.bytes.len() {
            return Err(VirtioBlockRequestError::MemoryOutOfRange { ipa, len });
        }
        Ok(offset..end)
    }

    pub(crate) fn read_bytes(
        &self,
        ipa: u64,
        len: usize,
    ) -> Result<Vec<u8>, VirtioBlockRequestError> {
        Ok(self.read_slice(ipa, len)?.to_vec())
    }

    pub(crate) fn read_slice(
        &self,
        ipa: u64,
        len: usize,
    ) -> Result<&[u8], VirtioBlockRequestError> {
        let range = self.range(ipa, len)?;
        Ok(&self.bytes[range])
    }

    pub(crate) fn read_array<const N: usize>(
        &self,
        ipa: u64,
    ) -> Result<[u8; N], VirtioBlockRequestError> {
        let mut bytes = [0u8; N];
        bytes.copy_from_slice(self.read_slice(ipa, N)?);
        Ok(bytes)
    }

    pub(crate) fn read_u16(&self, ipa: u64) -> Result<u16, VirtioBlockRequestError> {
        Ok(u16::from_le_bytes(self.read_array(ipa)?))
    }

    pub(crate) fn read_u32(&self, ipa: u64) -> Result<u32, VirtioBlockRequestError> {
        Ok(u32::from_le_bytes(self.read_array(ipa)?))
    }

    pub(crate) fn read_u64(&self, ipa: u64) -> Result<u64, VirtioBlockRequestError> {
        Ok(u64::from_le_bytes(self.read_array(ipa)?))
    }

    pub(crate) fn write_bytes(
        &mut self,
        ipa: u64,
        bytes: &[u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let range = self.range(ipa, bytes.len())?;
        self.bytes[range].copy_from_slice(bytes);
        Ok(())
    }

    pub(crate) fn write_u8(&mut self, ipa: u64, value: u8) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &[value])
    }

    pub(crate) fn write_u16(
        &mut self,
        ipa: u64,
        value: u16,
    ) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &value.to_le_bytes())
    }

    pub(crate) fn write_u32(
        &mut self,
        ipa: u64,
        value: u32,
    ) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &value.to_le_bytes())
    }

    pub(crate) fn write_u64(
        &mut self,
        ipa: u64,
        value: u64,
    ) -> Result<(), VirtioBlockRequestError> {
        self.write_bytes(ipa, &value.to_le_bytes())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VirtqDescriptor {
    pub(crate) addr: u64,
    pub(crate) len: u32,
    pub(crate) flags: u16,
    pub(crate) next: u16,
}

impl VirtqDescriptor {
    pub(crate) fn read(
        memory: &VirtioGuestMemory<'_>,
        table_ipa: u64,
        index: u16,
        queue_size: u16,
    ) -> Result<Self, VirtioBlockRequestError> {
        if index >= queue_size {
            return Err(VirtioBlockRequestError::DescriptorIndexOutOfRange { index, queue_size });
        }
        let descriptor_ipa = table_ipa + (u64::from(index) * VIRTQ_DESC_SIZE);
        Ok(Self {
            addr: memory.read_u64(descriptor_ipa)?,
            len: memory.read_u32(descriptor_ipa + 8)?,
            flags: memory.read_u16(descriptor_ipa + 12)?,
            next: memory.read_u16(descriptor_ipa + 14)?,
        })
    }

    pub(crate) fn write(
        &self,
        memory: &mut VirtioGuestMemory<'_>,
        table_ipa: u64,
        index: u16,
    ) -> Result<(), VirtioBlockRequestError> {
        let descriptor_ipa = table_ipa + (u64::from(index) * VIRTQ_DESC_SIZE);
        memory.write_u64(descriptor_ipa, self.addr)?;
        memory.write_u32(descriptor_ipa + 8, self.len)?;
        memory.write_u16(descriptor_ipa + 12, self.flags)?;
        memory.write_u16(descriptor_ipa + 14, self.next)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct VirtioBlockRequestCompletion {
    pub(crate) descriptor_index: u16,
    pub(crate) request_type: u32,
    pub(crate) sector: u64,
    pub(crate) data_bytes: u32,
    pub(crate) status: u8,
    pub(crate) used_index: u16,
    pub(crate) interrupt_status: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum VirtioBlockRequestError {
    QueueNotReady,
    InvalidQueueSize(u64),
    NoAvailableRequest,
    MemoryOutOfRange {
        ipa: u64,
        len: usize,
    },
    DescriptorIndexOutOfRange {
        index: u16,
        queue_size: u16,
    },
    DescriptorTooSmall {
        role: &'static str,
        len: u32,
    },
    MissingNextDescriptor(&'static str),
    UnexpectedDescriptorFlags {
        role: &'static str,
        flags: u16,
    },
    UnsupportedRequestType(u32),
    InvalidDataLength(u32),
    StorageOffsetOverflow {
        sector: u64,
    },
    StorageReadOutOfRange {
        offset: u64,
        len: usize,
        capacity: u64,
    },
    StorageOpenFailed {
        path: PathBuf,
        error: String,
    },
    StorageReadFailed {
        offset: u64,
        len: usize,
        error: String,
    },
    StorageWriteRejected {
        backing_kind: &'static str,
        offset: u64,
        len: usize,
    },
    StorageWriteOutOfRange {
        offset: u64,
        len: usize,
        capacity: u64,
    },
    StorageWriteFailed {
        offset: u64,
        len: usize,
        error: String,
    },
    StorageFlushFailed {
        error: String,
    },
    MissingBlockDeviceMetadata {
        ipa: u64,
    },
    MissingBlockBackingPath {
        role: &'static str,
        backing_kind: &'static str,
    },
    UnsupportedBlockBackingKind {
        role: &'static str,
        backing_kind: &'static str,
    },
    UnsupportedQueueNotifyValue {
        role: &'static str,
        value: u64,
    },
    UnexpectedQueueNotifyIpa {
        role: &'static str,
        ipa: u64,
    },
    MissingMmioDevice(&'static str),
    UnexpectedMmioAction {
        register: &'static str,
        action: MmioAction,
    },
}

impl VirtioBlockRequestError {
    pub(crate) fn render_blocker(&self) -> String {
        match self {
            Self::MissingMmioDevice(device) => format!("missing MMIO device: {device}"),
            Self::UnexpectedMmioAction { register, action } => {
                format!("unexpected MMIO action for {register}: {action:?}")
            }
            Self::StorageOpenFailed { path, error } => {
                format!(
                    "could not open host block backing {}: {error}",
                    path.display()
                )
            }
            Self::StorageReadFailed { offset, len, error } => {
                format!("host block backing read failed at {offset:#x} for {len:#x} bytes: {error}")
            }
            Self::StorageWriteRejected {
                backing_kind,
                offset,
                len,
            } => {
                format!("{backing_kind} rejected block write at {offset:#x} for {len:#x} bytes")
            }
            Self::StorageWriteFailed { offset, len, error } => {
                format!(
                    "host block backing write failed at {offset:#x} for {len:#x} bytes: {error}"
                )
            }
            Self::StorageWriteOutOfRange {
                offset,
                len,
                capacity,
            } => {
                format!(
                    "host block backing write out of range at {offset:#x} for {len:#x} bytes against capacity {capacity:#x}"
                )
            }
            Self::StorageFlushFailed { error } => {
                format!("host block backing flush failed: {error}")
            }
            Self::MissingBlockDeviceMetadata { ipa } => {
                format!("missing firmware block-device metadata for MMIO IPA {ipa:#x}")
            }
            Self::MissingBlockBackingPath { role, backing_kind } => {
                format!("missing {backing_kind} backing path for firmware block device {role}")
            }
            Self::UnsupportedBlockBackingKind { role, backing_kind } => {
                format!("unsupported backing kind {backing_kind} for firmware block device {role}")
            }
            Self::UnsupportedQueueNotifyValue { role, value } => {
                format!(
                    "unsupported queue_notify value {value:#x} for firmware block device {role}"
                )
            }
            Self::UnexpectedQueueNotifyIpa { role, ipa } => {
                format!("unexpected queue_notify IPA {ipa:#x} for firmware block device {role}")
            }
            error => format!("{error:?}"),
        }
    }
}

pub(crate) trait VirtioBlockStorageBackend {
    fn kind(&self) -> &'static str;
    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError>;

    fn write_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &[u8],
    ) -> Result<(), VirtioBlockRequestError> {
        Err(VirtioBlockRequestError::StorageWriteRejected {
            backing_kind: self.kind(),
            offset: byte_offset,
            len: buffer.len(),
        })
    }

    fn flush(&mut self) -> Result<(), VirtioBlockRequestError> {
        Ok(())
    }
}

pub(crate) struct SyntheticBlockStorageBackend;

impl VirtioBlockStorageBackend for SyntheticBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "synthetic-sector-pattern"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        let sector = byte_offset / VIRTIO_BLOCK_SECTOR_BYTES;
        let sector_offset = byte_offset % VIRTIO_BLOCK_SECTOR_BYTES;
        for (index, byte) in buffer.iter_mut().enumerate() {
            let offset = sector_offset
                .checked_add(u64::try_from(index).map_err(|_| {
                    VirtioBlockRequestError::StorageReadOutOfRange {
                        offset: byte_offset,
                        len,
                        capacity: u64::MAX,
                    }
                })?)
                .ok_or(VirtioBlockRequestError::StorageOffsetOverflow { sector })?;
            *byte = synthetic_block_byte(sector, offset as u32);
        }
        Ok(())
    }
}

pub(crate) struct FileBlockStorageBackend {
    pub(crate) file: File,
    pub(crate) capacity: u64,
}

impl FileBlockStorageBackend {
    pub(crate) fn open(path: &PathBuf) -> Result<Self, VirtioBlockRequestError> {
        let file =
            File::open(path).map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?;
        let capacity = file
            .metadata()
            .map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?
            .len();
        Ok(Self { file, capacity })
    }
}

impl VirtioBlockStorageBackend for FileBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "host-file"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        let end = byte_offset.checked_add(len as u64).ok_or(
            VirtioBlockRequestError::StorageReadOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            },
        )?;
        if end > self.capacity {
            return Err(VirtioBlockRequestError::StorageReadOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            });
        }
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })?;
        self.file
            .read_exact(buffer)
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })
    }
}

pub(crate) struct WritableHostFileBlockStorageBackend {
    pub(crate) file: File,
    pub(crate) capacity: u64,
}

impl WritableHostFileBlockStorageBackend {
    pub(crate) fn open(path: &PathBuf) -> Result<Self, VirtioBlockRequestError> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)
            .map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?;
        let capacity = file
            .metadata()
            .map_err(|error| VirtioBlockRequestError::StorageOpenFailed {
                path: path.clone(),
                error: error.to_string(),
            })?
            .len();
        Ok(Self { file, capacity })
    }

    pub(crate) fn checked_range(
        &self,
        byte_offset: u64,
        len: usize,
    ) -> Result<(), VirtioBlockRequestError> {
        let end = byte_offset.checked_add(len as u64).ok_or(
            VirtioBlockRequestError::StorageWriteOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            },
        )?;
        if end > self.capacity {
            return Err(VirtioBlockRequestError::StorageWriteOutOfRange {
                offset: byte_offset,
                len,
                capacity: self.capacity,
            });
        }
        Ok(())
    }
}

impl VirtioBlockStorageBackend for WritableHostFileBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "host-file-writable"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        self.checked_range(byte_offset, len)?;
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })?;
        self.file
            .read_exact(buffer)
            .map_err(|error| VirtioBlockRequestError::StorageReadFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })
    }

    fn write_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &[u8],
    ) -> Result<(), VirtioBlockRequestError> {
        let len = buffer.len();
        self.checked_range(byte_offset, len)?;
        self.file
            .seek(SeekFrom::Start(byte_offset))
            .map_err(|error| VirtioBlockRequestError::StorageWriteFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })?;
        self.file
            .write_all(buffer)
            .map_err(|error| VirtioBlockRequestError::StorageWriteFailed {
                offset: byte_offset,
                len,
                error: error.to_string(),
            })
    }

    fn flush(&mut self) -> Result<(), VirtioBlockRequestError> {
        self.file
            .sync_data()
            .map_err(|error| VirtioBlockRequestError::StorageFlushFailed {
                error: error.to_string(),
            })
    }
}

pub(crate) struct ReadOnlyIsoBlockStorageBackend {
    pub(crate) inner: FileBlockStorageBackend,
}

impl ReadOnlyIsoBlockStorageBackend {
    pub(crate) fn open(path: &PathBuf) -> Result<Self, VirtioBlockRequestError> {
        Ok(Self {
            inner: FileBlockStorageBackend::open(path)?,
        })
    }
}

impl VirtioBlockStorageBackend for ReadOnlyIsoBlockStorageBackend {
    fn kind(&self) -> &'static str {
        "host-iso-readonly"
    }

    fn read_exact_at(
        &mut self,
        byte_offset: u64,
        buffer: &mut [u8],
    ) -> Result<(), VirtioBlockRequestError> {
        self.inner.read_exact_at(byte_offset, buffer)
    }
}

pub(crate) fn synthetic_block_byte(sector: u64, offset: u32) -> u8 {
    sector.wrapping_add(u64::from(offset)) as u8
}

pub(crate) fn seed_synthetic_virtio_block_read_request(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    memory.write_u32(
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS,
        VIRTIO_BLK_T_IN,
    )?;
    memory.write_u32(VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS + 4, 0)?;
    memory.write_u64(
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS + 8,
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR,
    )?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_REQUEST_HEADER_ADDRESS,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 1,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 0)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS,
        len: VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES,
        flags: VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
        next: 2,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 1)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 2)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 0)
}

pub(crate) fn seed_synthetic_virtio_block_write_request(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    memory.write_u32(
        VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS,
        VIRTIO_BLK_T_OUT,
    )?;
    memory.write_u32(VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS + 4, 0)?;
    memory.write_u64(
        VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS + 8,
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR,
    )?;
    let mut data = vec![0_u8; VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize];
    for (index, byte) in data.iter_mut().enumerate() {
        *byte = 0xe0_u8.wrapping_add(index as u8);
    }
    memory.write_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS, &data)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_WRITE_HEADER_ADDRESS,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 4,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 3)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS,
        len: VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 5,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 4)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 5)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 2)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 6, 3)
}

#[cfg(test)]
pub(crate) fn seed_synthetic_virtio_block_write_request_as_first(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    seed_synthetic_virtio_block_write_request(memory)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 3)
}

pub(crate) fn seed_synthetic_virtio_block_flush_request(
    memory: &mut VirtioGuestMemory<'_>,
) -> Result<(), VirtioBlockRequestError> {
    memory.write_u32(
        VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS,
        VIRTIO_BLK_T_FLUSH,
    )?;
    memory.write_u32(VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS + 4, 0)?;
    memory.write_u64(
        VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS + 8,
        VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR,
    )?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_FLUSH_HEADER_ADDRESS,
        len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
        flags: VIRTQ_DESC_F_NEXT,
        next: 7,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 6)?;
    VirtqDescriptor {
        addr: VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS,
        len: VIRTIO_BLOCK_STATUS_BYTES,
        flags: VIRTQ_DESC_F_WRITE,
        next: 0,
    }
    .write(memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 7)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 3)?;
    memory.write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 8, 6)
}

pub(crate) fn write_virtio_block_mmio_bus(
    bus: &mut MmioBus,
    block_base: u64,
    register: &'static str,
    offset: u64,
    value: u64,
) -> Result<(), VirtioBlockRequestError> {
    let expected = value & 0xffff_ffff;
    let action = bus.dispatch(MmioAccess::write(block_base + offset, value, 4));
    match action {
        MmioAction::WriteAccepted { value, .. } if value == expected => Ok(()),
        action => Err(VirtioBlockRequestError::UnexpectedMmioAction { register, action }),
    }
}

pub(crate) fn run_virtio_block_request_model(
) -> Result<VirtioBlockRequestModelProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let completion = block.complete_next_available_block_request(&mut memory)?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    Ok(VirtioBlockRequestModelProbe {
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        completed_via_device_bus: true,
        completed: true,
        descriptor_index: Some(completion.descriptor_index),
        request_type: Some(completion.request_type),
        sector: Some(completion.sector),
        data_bytes: Some(completion.data_bytes),
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: Some(memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0]),
        used_index: Some(completion.used_index),
        used_len: Some(used_len),
        interrupt_status: Some(completion.interrupt_status),
        blockers: Vec::new(),
    })
}

pub(crate) fn run_virtio_block_file_backing(
    disk_path: PathBuf,
) -> Result<VirtioBlockFileBackingProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let mut backend = FileBlockStorageBackend::open(&disk_path)?;
    let backing_kind = backend.kind();
    let completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let byte_offset = completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: completion.sector,
        })?;
    Ok(VirtioBlockFileBackingProbe {
        disk_path,
        backing_kind,
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        completed_via_device_bus: true,
        completed: true,
        descriptor_index: Some(completion.descriptor_index),
        request_type: Some(completion.request_type),
        sector: Some(completion.sector),
        byte_offset: Some(byte_offset),
        data_bytes: Some(completion.data_bytes),
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: Some(memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0]),
        used_index: Some(completion.used_index),
        used_len: Some(used_len),
        interrupt_status: Some(completion.interrupt_status),
        blockers: Vec::new(),
    })
}

pub(crate) fn run_virtio_block_writable_file_backing(
    disk_path: PathBuf,
) -> Result<VirtioBlockWritableFileBackingProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let mut backend = WritableHostFileBlockStorageBackend::open(&disk_path)?;
    let backing_kind = backend.kind();
    block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let initial_read_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?;

    seed_synthetic_virtio_block_write_request(&mut memory)?;
    let write_completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let write_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 16)?;
    let write_byte_offset = write_completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: write_completion.sector,
        })?;
    let write_data_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS, 8)?;
    let write_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS, 1)?[0];

    seed_synthetic_virtio_block_flush_request(&mut memory)?;
    let flush_completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let flush_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 24)?;
    let flush_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS, 1)?[0];
    let interrupt_status = flush_completion.interrupt_status;
    drop(backend);

    let mut persisted_data_prefix = vec![0_u8; 8];
    let mut reopened = FileBlockStorageBackend::open(&disk_path)?;
    reopened.read_exact_at(write_byte_offset, &mut persisted_data_prefix)?;

    Ok(VirtioBlockWritableFileBackingProbe {
        disk_path,
        backing_kind,
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        initial_read_prefix,
        write_completed: true,
        write_request_type: Some(write_completion.request_type),
        write_sector: Some(write_completion.sector),
        write_byte_offset: Some(write_byte_offset),
        write_data_bytes: Some(write_completion.data_bytes),
        write_data_prefix,
        write_status: Some(write_status),
        write_used_index: Some(write_completion.used_index),
        write_used_len: Some(write_used_len),
        flush_completed: true,
        flush_request_type: Some(flush_completion.request_type),
        flush_status: Some(flush_status),
        flush_used_index: Some(flush_completion.used_index),
        flush_used_len: Some(flush_used_len),
        persisted_data_prefix,
        interrupt_status: Some(interrupt_status),
        blockers: Vec::new(),
    })
}

pub(crate) fn run_virtio_block_iso_backing(
    iso_path: PathBuf,
) -> Result<VirtioBlockIsoBackingProbe, VirtioBlockRequestError> {
    let guest_base = 0x4000_0000;
    let mut backing = vec![0_u8; 16 * 1024];
    let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
    let block_base = 0x5000_2000;
    let mut bus = MmioBus::default();
    bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
    for (register, offset, value) in [
        (
            "queue_num",
            VIRTIO_MMIO_QUEUE_NUM_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
        ),
        (
            "queue_desc_low",
            VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_desc_high",
            VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
        ),
        (
            "queue_driver_low",
            VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_driver_high",
            VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
        ),
        (
            "queue_device_low",
            VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
        ),
        (
            "queue_device_high",
            VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
        ),
        (
            "queue_ready",
            VIRTIO_MMIO_QUEUE_READY_OFFSET,
            VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
        ),
    ] {
        write_virtio_block_mmio_bus(&mut bus, block_base, register, offset, value)?;
    }

    seed_synthetic_virtio_block_read_request(&mut memory)?;

    let queue_notify_value = 0;
    write_virtio_block_mmio_bus(
        &mut bus,
        block_base,
        "queue_notify",
        VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
        queue_notify_value,
    )?;
    let block = bus.find_device_mut::<VirtioMmioBlockDevice>().ok_or(
        VirtioBlockRequestError::MissingMmioDevice("VirtIO-MMIO block"),
    )?;
    let queue_notified = block.queue_notify == queue_notify_value;
    let mut backend = ReadOnlyIsoBlockStorageBackend::open(&iso_path)?;
    let backing_kind = backend.kind();
    let completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let byte_offset = completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: completion.sector,
        })?;
    seed_synthetic_virtio_block_write_request(&mut memory)?;
    let write_completion =
        block.complete_next_available_block_request_from_backend(&mut memory, &mut backend)?;
    let write_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 16)?;
    Ok(VirtioBlockIsoBackingProbe {
        iso_path,
        backing_kind,
        media_mode: "read-only",
        configured_via_mmio: true,
        configured_via_mmio_bus: true,
        queue_notified,
        queue_notify_value: Some(block.queue_notify),
        completed_via_device_bus: true,
        completed: true,
        descriptor_index: Some(completion.descriptor_index),
        request_type: Some(completion.request_type),
        sector: Some(completion.sector),
        byte_offset: Some(byte_offset),
        data_bytes: Some(completion.data_bytes),
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: Some(memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0]),
        used_index: Some(completion.used_index),
        used_len: Some(used_len),
        interrupt_status: Some(completion.interrupt_status),
        readonly_write_rejected: write_completion.status == VIRTIO_BLK_S_IOERR,
        readonly_write_status: Some(write_completion.status),
        readonly_write_used_index: Some(write_completion.used_index),
        readonly_write_used_len: Some(write_used_len),
        readonly_write_interrupt_status: Some(write_completion.interrupt_status),
        blockers: Vec::new(),
    })
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairAccessTelemetry {
    pub(crate) kind: &'static str,
    pub(crate) direction: &'static str,
    pub(crate) address: Option<u64>,
    pub(crate) sysreg: Option<u16>,
    pub(crate) syndrome: Option<u64>,
}

impl Default for LowVectorPostRepairAccessTelemetry {
    fn default() -> Self {
        Self {
            kind: "not observed",
            direction: "not observed",
            address: None,
            sysreg: None,
            syndrome: None,
        }
    }
}

impl LowVectorPostRepairAccessTelemetry {
    pub(crate) fn observed(exit: &WindowsArmUefiFirmwareRunLoopExit) -> Self {
        let Some(syndrome) = exit.exit_syndrome else {
            return Self {
                kind: "not applicable",
                direction: "not applicable",
                ..Self::default()
            };
        };

        if let Some(access) = decode_mmio_data_abort(syndrome) {
            return Self {
                kind: "mmio",
                direction: access.access_name(),
                address: exit.exit_physical_address.or(exit.exit_virtual_address),
                sysreg: None,
                syndrome: Some(syndrome),
            };
        }

        if let Some(access) = decode_system_register_trap(syndrome) {
            return Self {
                kind: windows_arm_firmware_post_repair_sysreg_access_kind(access.sys_reg),
                direction: access.access_name(),
                address: None,
                sysreg: Some(access.sys_reg),
                syndrome: Some(syndrome),
            };
        }

        Self {
            kind: "exception",
            direction: "not applicable",
            address: None,
            sysreg: None,
            syndrome: Some(syndrome),
        }
    }
}

pub(crate) fn windows_arm_firmware_post_repair_sysreg_access_kind(sys_reg: u16) -> &'static str {
    if windows_arm_firmware_is_icc_sysreg(sys_reg) {
        "icc-sysreg"
    } else {
        "sysreg"
    }
}

pub(crate) fn windows_arm_firmware_is_icc_sysreg(sys_reg: u16) -> bool {
    matches!(
        sys_reg,
        ICC_PMR_EL1_SYSREG
            | ICC_IAR0_EL1_SYSREG
            | ICC_EOIR0_EL1_SYSREG
            | ICC_HPPIR0_EL1_SYSREG
            | ICC_BPR0_EL1_SYSREG
            | ICC_AP0R0_EL1_SYSREG
            | ICC_AP0R1_EL1_SYSREG
            | ICC_AP0R2_EL1_SYSREG
            | ICC_AP0R3_EL1_SYSREG
            | ICC_AP1R0_EL1_SYSREG
            | ICC_AP1R1_EL1_SYSREG
            | ICC_AP1R2_EL1_SYSREG
            | ICC_AP1R3_EL1_SYSREG
            | ICC_DIR_EL1_SYSREG
            | ICC_RPR_EL1_SYSREG
            | ICC_SGI1R_EL1_SYSREG
            | ICC_IAR1_EL1_SYSREG
            | ICC_EOIR1_EL1_SYSREG
            | ICC_HPPIR1_EL1_SYSREG
            | ICC_BPR1_EL1_SYSREG
            | ICC_CTLR_EL1_SYSREG
            | ICC_SRE_EL1_SYSREG
            | ICC_IGRPEN0_EL1_SYSREG
            | ICC_IGRPEN1_EL1_SYSREG
    )
}

pub(crate) fn windows_arm_firmware_sysreg_name(sys_reg: u16) -> &'static str {
    match sys_reg {
        ICC_PMR_EL1_SYSREG => "ICC_PMR_EL1",
        ICC_IAR0_EL1_SYSREG => "ICC_IAR0_EL1",
        ICC_EOIR0_EL1_SYSREG => "ICC_EOIR0_EL1",
        ICC_HPPIR0_EL1_SYSREG => "ICC_HPPIR0_EL1",
        ICC_BPR0_EL1_SYSREG => "ICC_BPR0_EL1",
        ICC_AP0R0_EL1_SYSREG => "ICC_AP0R0_EL1",
        ICC_AP0R1_EL1_SYSREG => "ICC_AP0R1_EL1",
        ICC_AP0R2_EL1_SYSREG => "ICC_AP0R2_EL1",
        ICC_AP0R3_EL1_SYSREG => "ICC_AP0R3_EL1",
        ICC_AP1R0_EL1_SYSREG => "ICC_AP1R0_EL1",
        ICC_AP1R1_EL1_SYSREG => "ICC_AP1R1_EL1",
        ICC_AP1R2_EL1_SYSREG => "ICC_AP1R2_EL1",
        ICC_AP1R3_EL1_SYSREG => "ICC_AP1R3_EL1",
        ICC_DIR_EL1_SYSREG => "ICC_DIR_EL1",
        ICC_RPR_EL1_SYSREG => "ICC_RPR_EL1",
        ICC_SGI1R_EL1_SYSREG => "ICC_SGI1R_EL1",
        ICC_IAR1_EL1_SYSREG => "ICC_IAR1_EL1",
        ICC_EOIR1_EL1_SYSREG => "ICC_EOIR1_EL1",
        ICC_HPPIR1_EL1_SYSREG => "ICC_HPPIR1_EL1",
        ICC_BPR1_EL1_SYSREG => "ICC_BPR1_EL1",
        ICC_CTLR_EL1_SYSREG => "ICC_CTLR_EL1",
        ICC_SRE_EL1_SYSREG => "ICC_SRE_EL1",
        ICC_IGRPEN0_EL1_SYSREG => "ICC_IGRPEN0_EL1",
        ICC_IGRPEN1_EL1_SYSREG => "ICC_IGRPEN1_EL1",
        _ => "unknown",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairUnhandledAccessTelemetry {
    pub(crate) observed: bool,
    pub(crate) index: Option<u32>,
    pub(crate) reason: Option<u32>,
    pub(crate) diagnosis: &'static str,
    pub(crate) pc: Option<u64>,
    pub(crate) syndrome: Option<u64>,
    pub(crate) kind: &'static str,
    pub(crate) access: &'static str,
    pub(crate) register: Option<u8>,
    pub(crate) value: Option<u64>,
    pub(crate) handler_result: &'static str,
    pub(crate) mmio_ipa: Option<u64>,
    pub(crate) mmio_width: Option<u8>,
    pub(crate) mmio_device_kind: &'static str,
    pub(crate) sysreg: Option<u16>,
    pub(crate) sysreg_name: &'static str,
    pub(crate) sysreg_op0: Option<u8>,
    pub(crate) sysreg_op1: Option<u8>,
    pub(crate) sysreg_crn: Option<u8>,
    pub(crate) sysreg_crm: Option<u8>,
    pub(crate) sysreg_op2: Option<u8>,
}

impl Default for LowVectorPostRepairUnhandledAccessTelemetry {
    fn default() -> Self {
        Self {
            observed: false,
            index: None,
            reason: None,
            diagnosis: "not observed",
            pc: None,
            syndrome: None,
            kind: "not observed",
            access: "not observed",
            register: None,
            value: None,
            handler_result: "not observed",
            mmio_ipa: None,
            mmio_width: None,
            mmio_device_kind: "not observed",
            sysreg: None,
            sysreg_name: "not observed",
            sysreg_op0: None,
            sysreg_op1: None,
            sysreg_crn: None,
            sysreg_crm: None,
            sysreg_op2: None,
        }
    }
}

impl LowVectorPostRepairUnhandledAccessTelemetry {
    pub(crate) fn mmio(
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedMmioDataAbort,
        ipa: u64,
        value: Option<u64>,
        handler_result: &'static str,
    ) -> Self {
        Self {
            observed: true,
            index: Some(exit.index),
            reason: exit.exit_reason,
            diagnosis: windows_arm_firmware_run_loop_exit_diagnosis(exit),
            pc: exit.pc_after_exit,
            syndrome: exit.exit_syndrome,
            kind: "mmio",
            access: access.access_name(),
            register: Some(access.register),
            value,
            handler_result,
            mmio_ipa: Some(ipa),
            mmio_width: Some(access.width),
            mmio_device_kind: windows_arm_firmware_mmio_device_kind_label(
                windows_arm_firmware_mmio_device_kind(block_devices, ipa),
            ),
            sysreg: None,
            sysreg_name: "not observed",
            sysreg_op0: None,
            sysreg_op1: None,
            sysreg_crn: None,
            sysreg_crm: None,
            sysreg_op2: None,
        }
    }

    pub(crate) fn sysreg(
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedSystemRegisterAccess,
        value: Option<u64>,
        handler_result: &'static str,
    ) -> Self {
        Self {
            observed: true,
            index: Some(exit.index),
            reason: exit.exit_reason,
            diagnosis: windows_arm_firmware_run_loop_exit_diagnosis(exit),
            pc: exit.pc_after_exit,
            syndrome: exit.exit_syndrome,
            kind: windows_arm_firmware_post_repair_sysreg_access_kind(access.sys_reg),
            access: access.access_name(),
            register: Some(access.register),
            value,
            handler_result,
            mmio_ipa: None,
            mmio_width: None,
            mmio_device_kind: "not observed",
            sysreg: Some(access.sys_reg),
            sysreg_name: windows_arm_firmware_sysreg_name(access.sys_reg),
            sysreg_op0: Some(access.op0),
            sysreg_op1: Some(access.op1),
            sysreg_crn: Some(access.crn),
            sysreg_crm: Some(access.crm),
            sysreg_op2: Some(access.op2),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairExitTelemetry {
    pub(crate) observed: bool,
    pub(crate) index: Option<u32>,
    pub(crate) reason: Option<u32>,
    pub(crate) diagnosis: &'static str,
    pub(crate) pc: Option<u64>,
    pub(crate) interaction_kind: &'static str,
    pub(crate) access: LowVectorPostRepairAccessTelemetry,
}

impl Default for LowVectorPostRepairExitTelemetry {
    fn default() -> Self {
        Self {
            observed: false,
            index: None,
            reason: None,
            diagnosis: "not observed",
            pc: None,
            interaction_kind: "not observed",
            access: LowVectorPostRepairAccessTelemetry::default(),
        }
    }
}

impl LowVectorPostRepairExitTelemetry {
    pub(crate) fn observed(
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
    ) -> Self {
        Self {
            observed: true,
            index: Some(exit.index),
            reason: exit.exit_reason,
            diagnosis: windows_arm_firmware_run_loop_exit_diagnosis(exit),
            pc: exit.pc_after_exit,
            interaction_kind: windows_arm_firmware_post_repair_interaction_kind(
                block_devices,
                exit,
            ),
            access: LowVectorPostRepairAccessTelemetry::observed(exit),
        }
    }

    pub(crate) fn is_device_interaction(&self) -> bool {
        windows_arm_firmware_post_repair_is_device_interaction(self.interaction_kind)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct LowVectorPostRepairTelemetry {
    pub(crate) continue_attempted: bool,
    pub(crate) unsupported_exit_observed: bool,
    pub(crate) unsupported_exit_reason: Option<u32>,
    pub(crate) unsupported_exit_diagnosis: &'static str,
    pub(crate) first_exit: LowVectorPostRepairExitTelemetry,
    pub(crate) first_device_interaction: LowVectorPostRepairExitTelemetry,
    pub(crate) first_unhandled_access: LowVectorPostRepairUnhandledAccessTelemetry,
}

impl Default for LowVectorPostRepairTelemetry {
    fn default() -> Self {
        Self {
            continue_attempted: false,
            unsupported_exit_observed: false,
            unsupported_exit_reason: None,
            unsupported_exit_diagnosis: "not observed",
            first_exit: LowVectorPostRepairExitTelemetry::default(),
            first_device_interaction: LowVectorPostRepairExitTelemetry::default(),
            first_unhandled_access: LowVectorPostRepairUnhandledAccessTelemetry::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LowVectorDiagnosticPageResumeTelemetry {
    pub(crate) attempted: bool,
    pub(crate) armed: bool,
    pub(crate) original_pc: Option<u64>,
    pub(crate) original_elr_el1: Option<u64>,
    pub(crate) original_esr_el1: Option<u64>,
    pub(crate) original_far_el1: Option<u64>,
    pub(crate) original_spsr_el1: Option<u64>,
    pub(crate) original_slot_bytes: Option<[u8; 12]>,
    pub(crate) target_instruction_word_before_eret: Option<u32>,
    pub(crate) target_stage1_leaf_descriptor_before_eret: Option<u64>,
    pub(crate) target_stage1_leaf_kind_before_eret: &'static str,
    pub(crate) target_is_installed_diagnostic_hvc_before_eret: bool,
    pub(crate) elr_el1_set_status: Option<i32>,
    pub(crate) spsr_el1_set_status: Option<i32>,
    pub(crate) cpsr_set_status: Option<i32>,
    pub(crate) pc_set_status: Option<i32>,
}

impl LowVectorDiagnosticPageResumeTelemetry {
    pub(crate) fn new() -> Self {
        Self {
            target_stage1_leaf_kind_before_eret: "not observed",
            ..Self::default()
        }
    }

    pub(crate) fn capture_original_context(&mut self, exit: &WindowsArmUefiFirmwareRunLoopExit) {
        self.original_pc = exit.pc_after_exit;
        self.original_elr_el1 = exit.elr_el1_after_exit;
        self.original_esr_el1 = exit.esr_el1_after_exit;
        self.original_far_el1 = exit.far_el1_after_exit;
        self.original_spsr_el1 = exit.spsr_el1_after_exit;
    }

    pub(crate) fn capture_diagnostic_slot_bytes(&mut self, original_slot_bytes: Option<[u8; 12]>) {
        self.original_slot_bytes = original_slot_bytes;
    }

    pub(crate) fn record_eret_target_snapshot(
        &mut self,
        instruction_word: Option<u32>,
        stage1_leaf_descriptor: Option<u64>,
        stage1_leaf_kind: &'static str,
    ) {
        self.target_instruction_word_before_eret = instruction_word;
        self.target_stage1_leaf_descriptor_before_eret = stage1_leaf_descriptor;
        self.target_stage1_leaf_kind_before_eret = stage1_leaf_kind;
        self.target_is_installed_diagnostic_hvc_before_eret =
            self.target_instruction_word_before_eret == Some(AARCH64_HVC_1_INSTRUCTION)
                && self.target_stage1_leaf_descriptor_before_eret
                    == Some(WINDOWS_ARM_LOW_VECTOR_DIAGNOSTIC_PAGE_DESCRIPTOR);
    }

    pub(crate) fn mark_attempted(&mut self) {
        self.attempted = true;
    }

    pub(crate) fn mark_armed(&mut self) {
        self.armed = true;
    }

    pub(crate) fn record_direct_resume_status(&mut self, cpsr_status: i32, pc_status: i32) {
        self.cpsr_set_status = Some(cpsr_status);
        self.pc_set_status = Some(pc_status);
    }

    pub(crate) fn record_eret_resume_status(
        &mut self,
        elr_status: i32,
        spsr_status: i32,
        pc_status: i32,
    ) {
        self.elr_el1_set_status = Some(elr_status);
        self.spsr_el1_set_status = Some(spsr_status);
        self.pc_set_status = Some(pc_status);
    }
}

impl LowVectorPostRepairTelemetry {
    pub(crate) fn mark_continue_attempted(&mut self) {
        self.continue_attempted = true;
    }

    pub(crate) fn observe_first_exit(
        &mut self,
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
    ) {
        if self.first_exit.observed {
            return;
        }

        self.first_exit = LowVectorPostRepairExitTelemetry::observed(block_devices, exit);
    }

    pub(crate) fn observe_device_interaction(
        &mut self,
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
    ) {
        if self.first_device_interaction.observed {
            return;
        }

        let candidate = LowVectorPostRepairExitTelemetry::observed(block_devices, exit);
        if !candidate.is_device_interaction() {
            return;
        }

        self.first_device_interaction = candidate;
    }

    pub(crate) fn first_device_interaction_is(&self, index: u32) -> bool {
        self.first_device_interaction.observed && self.first_device_interaction.index == Some(index)
    }

    pub(crate) fn observe_unsupported_exit(&mut self, exit: &WindowsArmUefiFirmwareRunLoopExit) {
        self.unsupported_exit_observed = true;
        self.unsupported_exit_reason = exit.exit_reason;
        self.unsupported_exit_diagnosis = windows_arm_firmware_run_loop_exit_diagnosis(exit);
    }

    pub(crate) fn observe_unhandled_mmio_access(
        &mut self,
        block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedMmioDataAbort,
        ipa: u64,
        value: Option<u64>,
        handler_result: &'static str,
    ) {
        if self.first_unhandled_access.observed {
            return;
        }

        self.first_unhandled_access = LowVectorPostRepairUnhandledAccessTelemetry::mmio(
            block_devices,
            exit,
            access,
            ipa,
            value,
            handler_result,
        );
    }

    pub(crate) fn observe_unhandled_sysreg_access(
        &mut self,
        exit: &WindowsArmUefiFirmwareRunLoopExit,
        access: DecodedSystemRegisterAccess,
        value: Option<u64>,
        handler_result: &'static str,
    ) {
        if self.first_unhandled_access.observed {
            return;
        }

        self.first_unhandled_access = LowVectorPostRepairUnhandledAccessTelemetry::sysreg(
            exit,
            access,
            value,
            handler_result,
        );
    }
}

pub(crate) fn windows_arm_firmware_post_repair_is_device_interaction(kind: &str) -> bool {
    kind == "sysreg:trap" || kind.starts_with("mmio:")
}

pub(crate) fn windows_arm_firmware_post_repair_interaction_kind(
    block_devices: &[WindowsArmVirtioBlockDeviceMetadata],
    exit: &WindowsArmUefiFirmwareRunLoopExit,
) -> &'static str {
    if exit.run_status != Some(HV_SUCCESS_VALUE) {
        return "hv-run-failure";
    }

    match exit.exit_reason {
        Some(HV_EXIT_REASON_CANCELED_VALUE) => "watchdog-cancel",
        Some(HV_EXIT_REASON_VTIMER_ACTIVATED_VALUE) => "vtimer",
        Some(HV_EXIT_REASON_EXCEPTION_VALUE) => {
            let Some(syndrome) = exit.exit_syndrome else {
                return "exception:missing-syndrome";
            };
            if decode_mmio_data_abort(syndrome).is_some() {
                let Some(ipa) = exit.exit_physical_address.or(exit.exit_virtual_address) else {
                    return "mmio:missing-address";
                };
                return match windows_arm_firmware_mmio_device_kind(block_devices, ipa) {
                    Some(WindowsArmFirmwareMmioDeviceKind::Pl011) => "mmio:pl011",
                    Some(WindowsArmFirmwareMmioDeviceKind::Pl031) => "mmio:pl031",
                    Some(WindowsArmFirmwareMmioDeviceKind::GicDistributor) => "mmio:gicd",
                    Some(WindowsArmFirmwareMmioDeviceKind::GicRedistributor) => "mmio:gicr",
                    Some(WindowsArmFirmwareMmioDeviceKind::VirtioInstallerIso) => {
                        "mmio:virtio-installer-iso"
                    }
                    Some(WindowsArmFirmwareMmioDeviceKind::VirtioTargetDisk) => {
                        "mmio:virtio-target-disk"
                    }
                    None if windows_arm_device_mmio_contains(ipa) => {
                        "mmio:windows-device-window-unclassified"
                    }
                    None => "mmio:outside-windows-device-window",
                };
            }
            if decode_system_register_trap(syndrome).is_some() {
                return "sysreg:trap";
            }
            "exception:non-mmio"
        }
        Some(_) => "exit:unsupported-reason",
        None => "exit:missing-info",
    }
}

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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct GicV3CpuInterfaceState {
    pub(crate) sre: u64,
    pub(crate) ctlr: u64,
    pub(crate) priority_mask: u8,
    pub(crate) binary_point0: u8,
    pub(crate) binary_point1: u8,
    pub(crate) group0_enabled: bool,
    pub(crate) group1_enabled: bool,
    pub(crate) active_priority0: [u32; 4],
    pub(crate) active_priority1: [u32; 4],
    pub(crate) active_group1: Vec<GicV3ActiveInterrupt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GicV3CpuInterfaceAction {
    Read(u64),
    Write { refresh_level_sources: bool },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct GicV3CpuInterfaceIrqLineSnapshot {
    pub(crate) group1_enabled: bool,
    pub(crate) priority_mask: u8,
    pub(crate) running_priority: u8,
    pub(crate) priority_threshold: u8,
    pub(crate) pending_intid: u32,
    pub(crate) irq_line_should_assert: bool,
}

impl GicV3CpuInterfaceState {
    pub(crate) fn new() -> Self {
        Self {
            // Report system-register access as enabled for the guest-visible CPU interface.
            sre: 0x7,
            ctlr: 0,
            priority_mask: 0xff,
            binary_point0: 0,
            binary_point1: 0,
            group0_enabled: false,
            group1_enabled: false,
            active_priority0: [0; 4],
            active_priority1: [0; 4],
            active_group1: Vec::new(),
        }
    }

    pub(crate) fn irq_line_snapshot(&self, bus: &mut MmioBus) -> GicV3CpuInterfaceIrqLineSnapshot {
        let running_priority = self.running_priority();
        let priority_threshold = self.priority_mask.min(running_priority);
        let pending_intid = pending_windows_arm_firmware_gic_irq(bus, priority_threshold);
        let irq_line_should_assert =
            self.group1_enabled && pending_intid != GICV3_SPURIOUS_INTERRUPT_ID;

        GicV3CpuInterfaceIrqLineSnapshot {
            group1_enabled: self.group1_enabled,
            priority_mask: self.priority_mask,
            running_priority,
            priority_threshold,
            pending_intid,
            irq_line_should_assert,
        }
    }

    #[cfg(test)]
    pub(crate) fn irq_line_should_assert(&self, bus: &mut MmioBus) -> bool {
        self.irq_line_snapshot(bus).irq_line_should_assert
    }

    pub(crate) fn mask_write(value: u64) -> u64 {
        value & 0xffff_ffff
    }

    pub(crate) fn acknowledge_group1(&mut self, bus: &mut MmioBus) -> u32 {
        if !self.group1_enabled {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        }
        let Some(interrupt) =
            acknowledge_windows_arm_firmware_gic_irq(bus, self.group1_priority_threshold())
        else {
            return GICV3_SPURIOUS_INTERRUPT_ID;
        };
        self.active_group1.push(GicV3ActiveInterrupt {
            interrupt_id: interrupt.interrupt_id,
            priority: interrupt.priority,
            priority_dropped: false,
        });
        interrupt.interrupt_id
    }

    pub(crate) fn highest_pending_group1(&self, bus: &mut MmioBus) -> u32 {
        if self.group1_enabled {
            pending_windows_arm_firmware_gic_irq(bus, self.group1_priority_threshold())
        } else {
            GICV3_SPURIOUS_INTERRUPT_ID
        }
    }

    pub(crate) fn running_priority(&self) -> u8 {
        self.active_group1
            .iter()
            .filter(|interrupt| !interrupt.priority_dropped)
            .map(|interrupt| interrupt.priority)
            .min()
            .unwrap_or(0xff)
    }

    pub(crate) fn group1_priority_threshold(&self) -> u8 {
        self.priority_mask.min(self.running_priority())
    }

    pub(crate) fn active_priority_register_index(sys_reg: u16) -> Option<(bool, usize)> {
        match sys_reg {
            ICC_AP0R0_EL1_SYSREG => Some((false, 0)),
            ICC_AP0R1_EL1_SYSREG => Some((false, 1)),
            ICC_AP0R2_EL1_SYSREG => Some((false, 2)),
            ICC_AP0R3_EL1_SYSREG => Some((false, 3)),
            ICC_AP1R0_EL1_SYSREG => Some((true, 0)),
            ICC_AP1R1_EL1_SYSREG => Some((true, 1)),
            ICC_AP1R2_EL1_SYSREG => Some((true, 2)),
            ICC_AP1R3_EL1_SYSREG => Some((true, 3)),
            _ => None,
        }
    }

    pub(crate) fn eoi_mode(&self) -> bool {
        self.ctlr & ICC_CTLR_EL1_EOIMODE != 0
    }

    pub(crate) fn group1_interrupt_id_from_write(value: u64) -> Option<u32> {
        let interrupt_id = (value & 0x00ff_ffff) as u32;
        if interrupt_id == GICV3_SPURIOUS_INTERRUPT_ID {
            None
        } else {
            Some(interrupt_id)
        }
    }

    pub(crate) fn priority_drop_group1(&mut self, value: u64) {
        let Some(interrupt_id) = Self::group1_interrupt_id_from_write(value) else {
            return;
        };
        if let Some(active) = self
            .active_group1
            .iter_mut()
            .rfind(|active| active.interrupt_id == interrupt_id)
        {
            active.priority_dropped = true;
        }
    }

    pub(crate) fn deactivate_group1(&mut self, bus: &mut MmioBus, value: u64) -> bool {
        let Some(interrupt_id) = Self::group1_interrupt_id_from_write(value) else {
            return false;
        };
        if let Some(position) = self
            .active_group1
            .iter()
            .rposition(|active| active.interrupt_id == interrupt_id)
        {
            self.active_group1.remove(position);
        }
        end_windows_arm_firmware_gic_irq(bus, interrupt_id)
    }

    pub(crate) fn write_eoir_group1(&mut self, bus: &mut MmioBus, value: u64) -> bool {
        self.priority_drop_group1(value);
        if self.eoi_mode() {
            false
        } else {
            self.deactivate_group1(bus, value)
        }
    }

    pub(crate) fn write_dir_group1(&mut self, bus: &mut MmioBus, value: u64) -> bool {
        self.deactivate_group1(bus, value)
    }

    pub(crate) fn handle_system_register_access(
        &mut self,
        bus: &mut MmioBus,
        access: DecodedSystemRegisterAccess,
        write_value: Option<u64>,
    ) -> Option<GicV3CpuInterfaceAction> {
        match (access.is_read, access.sys_reg) {
            (true, ICC_SRE_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(self.sre)),
            (false, ICC_SRE_EL1_SYSREG) => {
                self.sre = Self::mask_write(write_value?) | 1;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_CTLR_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(self.ctlr)),
            (false, ICC_CTLR_EL1_SYSREG) => {
                self.ctlr = Self::mask_write(write_value?);
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_PMR_EL1_SYSREG) => {
                Some(GicV3CpuInterfaceAction::Read(u64::from(self.priority_mask)))
            }
            (false, ICC_PMR_EL1_SYSREG) => {
                self.priority_mask = (write_value? & 0xff) as u8;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_RPR_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.running_priority(),
            ))),
            (true, ICC_BPR0_EL1_SYSREG) => {
                Some(GicV3CpuInterfaceAction::Read(u64::from(self.binary_point0)))
            }
            (false, ICC_BPR0_EL1_SYSREG) => {
                self.binary_point0 = (write_value? & 0x7) as u8;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_BPR1_EL1_SYSREG) => {
                Some(GicV3CpuInterfaceAction::Read(u64::from(self.binary_point1)))
            }
            (false, ICC_BPR1_EL1_SYSREG) => {
                self.binary_point1 = (write_value? & 0x7) as u8;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_IGRPEN0_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.group0_enabled as u8,
            ))),
            (false, ICC_IGRPEN0_EL1_SYSREG) => {
                self.group0_enabled = (write_value? & 1) != 0;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_IGRPEN1_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.group1_enabled as u8,
            ))),
            (false, ICC_IGRPEN1_EL1_SYSREG) => {
                self.group1_enabled = (write_value? & 1) != 0;
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources: false,
                })
            }
            (true, ICC_HPPIR1_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.highest_pending_group1(bus),
            ))),
            (true, ICC_IAR1_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Read(u64::from(
                self.acknowledge_group1(bus),
            ))),
            (true, ICC_HPPIR0_EL1_SYSREG | ICC_IAR0_EL1_SYSREG) => Some(
                GicV3CpuInterfaceAction::Read(u64::from(GICV3_SPURIOUS_INTERRUPT_ID)),
            ),
            (false, ICC_EOIR0_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            }),
            (false, ICC_EOIR1_EL1_SYSREG) => {
                let refresh_level_sources = self.write_eoir_group1(bus, write_value?);
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources,
                })
            }
            (false, ICC_DIR_EL1_SYSREG) => {
                let refresh_level_sources = self.write_dir_group1(bus, write_value?);
                Some(GicV3CpuInterfaceAction::Write {
                    refresh_level_sources,
                })
            }
            (false, ICC_SGI1R_EL1_SYSREG) => Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            }),
            (is_read, sys_reg) => {
                let (group1, index) = Self::active_priority_register_index(sys_reg)?;
                if is_read {
                    let value = if group1 {
                        self.active_priority1[index]
                    } else {
                        self.active_priority0[index]
                    };
                    Some(GicV3CpuInterfaceAction::Read(u64::from(value)))
                } else {
                    let value = Self::mask_write(write_value?) as u32;
                    if group1 {
                        self.active_priority1[index] = value;
                    } else {
                        self.active_priority0[index] = value;
                    }
                    Some(GicV3CpuInterfaceAction::Write {
                        refresh_level_sources: false,
                    })
                }
            }
        }
    }
}

pub(crate) fn complete_probe_virtio_block_request(
    block: &mut VirtioMmioBlockDevice,
    memory: &mut VirtioGuestMemory<'_>,
    backing: VirtioBlockProbeBackingRef<'_>,
) -> Result<VirtioBlockProbeCompletion, VirtioBlockRequestError> {
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
    let used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let data_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?;
    let status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0];

    Ok(VirtioBlockProbeCompletion {
        completion,
        backing_kind,
        byte_offset,
        used_len,
        data_prefix,
        status,
    })
}

pub(crate) fn complete_probe_virtio_block_writable_file_requests(
    block: &mut VirtioMmioBlockDevice,
    memory: &mut VirtioGuestMemory<'_>,
    path: &PathBuf,
) -> Result<VirtioBlockWritableProbeCompletion, VirtioBlockRequestError> {
    let mut backend = WritableHostFileBlockStorageBackend::open(path)?;
    let backing_kind = backend.kind();
    let initial_completion =
        block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
    let initial_byte_offset = initial_completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: initial_completion.sector,
        })?;
    let initial_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)?;
    let initial_read = VirtioBlockProbeCompletion {
        completion: initial_completion,
        backing_kind,
        byte_offset: initial_byte_offset,
        used_len: initial_used_len,
        data_prefix: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)?,
        status: memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_STATUS_ADDRESS, 1)?[0],
    };

    seed_synthetic_virtio_block_write_request(memory)?;
    let write_completion =
        block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
    let write_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 16)?;
    let write_byte_offset = write_completion
        .sector
        .checked_mul(VIRTIO_BLOCK_SECTOR_BYTES)
        .ok_or(VirtioBlockRequestError::StorageOffsetOverflow {
            sector: write_completion.sector,
        })?;
    let write_data_prefix = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_DATA_ADDRESS, 8)?;
    let write_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_WRITE_STATUS_ADDRESS, 1)?[0];

    seed_synthetic_virtio_block_flush_request(memory)?;
    let flush_completion =
        block.complete_next_available_block_request_from_backend(memory, &mut backend)?;
    let flush_used_len = memory.read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 24)?;
    let flush_status = memory.read_bytes(VIRTIO_BLOCK_SYNTHETIC_FLUSH_STATUS_ADDRESS, 1)?[0];
    drop(backend);

    let mut persisted_data_prefix = vec![0_u8; 8];
    let mut reopened = FileBlockStorageBackend::open(path)?;
    reopened.read_exact_at(write_byte_offset, &mut persisted_data_prefix)?;

    Ok(VirtioBlockWritableProbeCompletion {
        initial_read,
        write_completion,
        write_byte_offset,
        write_used_len,
        write_data_prefix,
        write_status,
        flush_completion,
        flush_used_len,
        flush_status,
        persisted_data_prefix,
    })
}

impl MmioDevice for VirtioMmioBlockDevice {
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }

    fn range(&self) -> MmioRange {
        MmioRange {
            start: self.base_ipa,
            bytes: VIRTIO_MMIO_REGISTER_WINDOW_BYTES,
        }
    }

    fn handle(&mut self, access: MmioAccess) -> MmioAction {
        let offset = access.ipa.saturating_sub(self.base_ipa);
        match (access.kind, offset, access.value) {
            (MmioAccessKind::Read, VIRTIO_MMIO_MAGIC_VALUE_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_VERSION_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_VERSION_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_DEVICE_ID_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_ID_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_VENDOR_ID_OFFSET, None) => {
                MmioAction::ReadValue(VIRTIO_MMIO_VENDOR_ID_VALUE)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_DEVICE_FEATURES_OFFSET, None) => {
                MmioAction::ReadValue(self.device_features)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, None) => {
                MmioAction::ReadValue(self.driver_features)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_SEL_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_select)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_num_max)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_NUM_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_num)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_READY_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_ready)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET, None) => {
                MmioAction::ReadValue(self.interrupt_status)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_STATUS_OFFSET, None) => {
                MmioAction::ReadValue(self.status)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_desc & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_desc >> 32)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_driver & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_driver >> 32)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_device & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.queue_device >> 32)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_CONFIG_GENERATION_OFFSET, None) => {
                MmioAction::ReadValue(self.config_generation)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET, None) => {
                MmioAction::ReadValue(self.capacity_sectors & 0xffff_ffff)
            }
            (MmioAccessKind::Read, VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET, None) => {
                MmioAction::ReadValue(self.capacity_sectors >> 32)
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.driver_features = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_SEL_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_select = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_NUM_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_num = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_READY_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_ready = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_notify = value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_INTERRUPT_ACK_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.interrupt_ack = value;
                self.interrupt_status &= !value;
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_STATUS_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                if value == 0 {
                    self.reset_driver_state();
                } else {
                    self.status = value;
                }
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_desc = Self::replace_low_32(self.queue_desc, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_desc = Self::replace_high_32(self.queue_desc, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_driver = Self::replace_low_32(self.queue_driver, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_driver = Self::replace_high_32(self.queue_driver, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_device = Self::replace_low_32(self.queue_device, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            (MmioAccessKind::Write, VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET, Some(value)) => {
                let value = Self::mask_value(value, access.width);
                self.queue_device = Self::replace_high_32(self.queue_device, value, access.width);
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            }
            _ => MmioAction::Unhandled,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn configure_virtio_block_queue_on_bus(bus: &mut MmioBus, block_base: u64) {
        for (register, offset, value) in [
            (
                "queue_num",
                VIRTIO_MMIO_QUEUE_NUM_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
            ),
            (
                "queue_desc_low",
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                "queue_desc_high",
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                "queue_driver_low",
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                "queue_driver_high",
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                "queue_device_low",
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                "queue_device_high",
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                "queue_ready",
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
        ] {
            write_virtio_block_mmio_bus(bus, block_base, register, offset, value).unwrap();
        }
    }

    fn sysreg_trap_syndrome(
        is_read: bool,
        register: u8,
        op0: u8,
        op1: u8,
        crn: u8,
        crm: u8,
        op2: u8,
    ) -> u64 {
        (AARCH64_SYSREG_TRAP_EXCEPTION_CLASS << 26)
            | (u64::from(op0) << 20)
            | (u64::from(op2) << 17)
            | (u64::from(op1) << 14)
            | (u64::from(crn) << 10)
            | (u64::from(register) << 5)
            | (u64::from(crm) << 1)
            | u64::from(is_read as u8)
    }

    fn gic_cpu_write(
        cpu: &mut GicV3CpuInterfaceState,
        bus: &mut MmioBus,
        sys_reg: u16,
        value: u64,
    ) -> Option<GicV3CpuInterfaceAction> {
        cpu.handle_system_register_access(
            bus,
            DecodedSystemRegisterAccess {
                is_read: false,
                register: 0,
                sys_reg,
                op0: 3,
                op1: 0,
                crn: 0,
                crm: 0,
                op2: 0,
            },
            Some(value),
        )
    }

    fn gic_cpu_read(
        cpu: &mut GicV3CpuInterfaceState,
        bus: &mut MmioBus,
        sys_reg: u16,
    ) -> Option<GicV3CpuInterfaceAction> {
        cpu.handle_system_register_access(
            bus,
            DecodedSystemRegisterAccess {
                is_read: true,
                register: 1,
                sys_reg,
                op0: 3,
                op1: 0,
                crn: 0,
                crm: 0,
                op2: 0,
            },
            None,
        )
    }

    #[test]
    fn firmware_mmio_bus_uses_windows_device_window_layout() {
        let mut bus = windows_arm_firmware_mmio_bus();

        assert_eq!(bus.device_count(), 6);
        assert!(windows_arm_device_mmio_contains(WINDOWS_ARM_PL011_MMIO_IPA));
        assert!(windows_arm_device_mmio_contains(WINDOWS_ARM_PL031_MMIO_IPA));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA
        ));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA
        ));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
        ));
        assert!(windows_arm_device_mmio_contains(
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
        ));
        assert!(!windows_arm_device_mmio_contains(WINDOWS_ARM_GUEST_RAM_IPA));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_PL011_MMIO_IPA + PL011_FR_OFFSET,
                4
            )),
            MmioAction::ReadValue(WINDOWS_ARM_PL011_FLAG_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(WINDOWS_ARM_PL011_MMIO_IPA, 0x141, 4)),
            MmioAction::WriteAccepted {
                value: 0x141,
                byte: 0x41
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(WINDOWS_ARM_PL031_MMIO_IPA, 4)),
            MmioAction::ReadValue(WINDOWS_ARM_PL031_READ_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_TYPER_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICD_TYPER_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IIDR_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA + GICR_TYPER_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICR_TYPER_VALUE & 0xffff_ffff)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA + GICR_IIDR_OFFSET,
                4
            )),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_BLK_F_RO)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_MAGIC_VALUE_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_MAGIC_VALUE)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE)
        );
    }

    #[test]
    fn gicv3_distributor_mmio_skeleton_tracks_common_firmware_registers() {
        let mut gic = GicV3DistributorDevice::new(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_TYPER_OFFSET, 4)),
            MmioAction::ReadValue(GICD_TYPER_VALUE)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_IIDR_OFFSET, 4)),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_STATUSR_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + GICD_STATUSR_OFFSET, 0xff, 4)),
            MmioAction::WriteAccepted {
                value: 0xff,
                byte: 0xff
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_STATUSR_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + GICD_CTLR_OFFSET, 0x13, 4)),
            MmioAction::WriteAccepted {
                value: 0x13,
                byte: 0x13
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_CTLR_OFFSET, 4)),
            MmioAction::ReadValue(0x13)
        );

        let spi_enable_offset = GICD_ISENABLER_BASE_OFFSET + 4;
        let spi_clear_offset = GICD_ICENABLER_BASE_OFFSET + 4;
        let spi_group_offset = GICD_IGROUPR_BASE_OFFSET + 4;
        let spi_group_modifier_offset = GICD_IGRPMODR_BASE_OFFSET + 4;
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_group_modifier_offset, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_group_modifier_offset, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_enable_offset, 0x9, 4)),
            MmioAction::WriteAccepted {
                value: 0x9,
                byte: 0x9
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
            MmioAction::ReadValue(0x9)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_clear_offset, 0x1, 4)),
            MmioAction::WriteAccepted {
                value: 0x1,
                byte: 0x1
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
            MmioAction::ReadValue(0x8)
        );
        assert_eq!(
            GicV3DistributorDevice::spi_interrupt_id(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI),
            Some(34)
        );
        assert_eq!(GicV3DistributorDevice::interrupt_bit(34), Some((1, 0x4)));
        assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_enable_offset, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_enable_offset, 4)),
            MmioAction::ReadValue(0xc)
        );
        assert_eq!(
            gic.set_spi_pending(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI, true),
            Some(())
        );
        assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_group_offset, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4
            }
        );
        assert!(gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));
        assert_eq!(
            gic.set_spi_pending(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI, false),
            Some(())
        );
        assert!(!gic.spi_irq_line_assertable(WINDOWS_ARM_VIRTIO_INSTALLER_ISO_SPI));

        let spi_pending_set_offset = GICD_ISPENDR_BASE_OFFSET + 4;
        let spi_pending_clear_offset = GICD_ICPENDR_BASE_OFFSET + 4;
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_pending_set_offset, 0x2, 4)),
            MmioAction::WriteAccepted {
                value: 0x2,
                byte: 0x2
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_pending_set_offset, 4)),
            MmioAction::ReadValue(0x2)
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + spi_pending_clear_offset, 0x2, 4)),
            MmioAction::WriteAccepted {
                value: 0x2,
                byte: 0x2
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + spi_pending_set_offset, 4)),
            MmioAction::ReadValue(0)
        );

        let priority_byte_offset = GICD_IPRIORITYR_BASE_OFFSET + 35;
        assert_eq!(
            gic.handle(MmioAccess::write(base + priority_byte_offset, 0x44, 1)),
            MmioAction::WriteAccepted {
                value: 0x44,
                byte: 0x44
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + priority_byte_offset, 1)),
            MmioAction::ReadValue(0x44)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_IPRIORITYR_BASE_OFFSET + 32, 4)),
            MmioAction::ReadValue(0x44a0_a0a0)
        );

        let router32 = GICD_IROUTER_BASE_OFFSET + (32 * 8);
        assert_eq!(
            gic.handle(MmioAccess::write(base + router32, 0x1122_3344, 4)),
            MmioAction::WriteAccepted {
                value: 0x1122_3344,
                byte: 0x44
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::write(base + router32 + 4, 0x5566_7788, 4)),
            MmioAction::WriteAccepted {
                value: 0x5566_7788,
                byte: 0x88
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + router32, 4)),
            MmioAction::ReadValue(0x1122_3344)
        );
        assert_eq!(
            gic.handle(MmioAccess::read(base + router32 + 4, 4)),
            MmioAction::ReadValue(0x5566_7788)
        );
    }

    #[test]
    fn gicv3_distributor_selects_pending_spi_by_priority_not_lowest_intid() {
        let mut gic = GicV3DistributorDevice::new(WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            gic.handle(MmioAccess::write(
                base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        for register_base in [
            GICD_IGROUPR_BASE_OFFSET,
            GICD_ISENABLER_BASE_OFFSET,
            GICD_ISPENDR_BASE_OFFSET,
        ] {
            assert_eq!(
                gic.handle(MmioAccess::write(base + register_base + 4, 0xc, 4)),
                MmioAction::WriteAccepted {
                    value: 0xc,
                    byte: 0xc,
                }
            );
        }
        assert_eq!(
            gic.handle(MmioAccess::write(
                base + GICD_IPRIORITYR_BASE_OFFSET + 34,
                0xa0,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0xa0,
                byte: 0xa0,
            }
        );
        assert_eq!(
            gic.handle(MmioAccess::write(
                base + GICD_IPRIORITYR_BASE_OFFSET + 35,
                0x20,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0x20,
                byte: 0x20,
            }
        );

        assert_eq!(gic.pending_interrupt_id_for_cpu(0xff), Some(35));
        assert_eq!(gic.acknowledge_pending_interrupt(0xff), 35);
        assert_eq!(
            gic.handle(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x8)
        );
        assert_eq!(gic.pending_interrupt_id_for_cpu(0xff), Some(34));
    }

    #[test]
    fn gicv3_redistributor_mmio_skeleton_tracks_waker_and_ppi_state() {
        let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_TYPER_OFFSET, 4)),
            MmioAction::ReadValue(GICR_TYPER_VALUE & 0xffff_ffff)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_IIDR_OFFSET, 4)),
            MmioAction::ReadValue(GICV3_IIDR_VALUE)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_STATUSR_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(base + GICR_STATUSR_OFFSET, 0x80, 4)),
            MmioAction::WriteAccepted {
                value: 0x80,
                byte: 0x80
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_WAKER_OFFSET,
                GICR_WAKER_PROCESSOR_SLEEP,
                4
            )),
            MmioAction::WriteAccepted {
                value: GICR_WAKER_PROCESSOR_SLEEP,
                byte: GICR_WAKER_PROCESSOR_SLEEP as u8
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
            MmioAction::ReadValue(GICR_WAKER_PROCESSOR_SLEEP | GICR_WAKER_CHILDREN_ASLEEP)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(base + GICR_WAKER_OFFSET, 0, 4)),
            MmioAction::WriteAccepted { value: 0, byte: 0 }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_WAKER_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );

        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISENABLER0_OFFSET,
                1 << 13,
                4
            )),
            MmioAction::WriteAccepted {
                value: 1 << 13,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISENABLER0_OFFSET, 4)),
            MmioAction::ReadValue(1 << 13)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ICENABLER0_OFFSET,
                1 << 13,
                4
            )),
            MmioAction::WriteAccepted {
                value: 1 << 13,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISENABLER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IGRPMODR0_OFFSET,
                1 << 13,
                4
            )),
            MmioAction::WriteAccepted {
                value: 1 << 13,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_IGRPMODR0_OFFSET, 4)),
            MmioAction::ReadValue(1 << 13)
        );

        let priority_byte_offset = GICR_SGI_IPRIORITYR_BASE_OFFSET + 13;
        assert_eq!(
            gicr.handle(MmioAccess::write(base + priority_byte_offset, 0x55, 1)),
            MmioAction::WriteAccepted {
                value: 0x55,
                byte: 0x55
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + priority_byte_offset, 1)),
            MmioAction::ReadValue(0x55)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(
                base + GICR_SGI_IPRIORITYR_BASE_OFFSET + 12,
                4
            )),
            MmioAction::ReadValue(0xa0a0_55a0)
        );

        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_PROPBASER_OFFSET,
                0x4000_0000,
                4
            )),
            MmioAction::WriteAccepted {
                value: 0x4000_0000,
                byte: 0
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_PROPBASER_OFFSET, 4)),
            MmioAction::ReadValue(0x4000_0000)
        );
    }

    #[test]
    fn gicv3_redistributor_tracks_virtual_timer_ppi_delivery_state() {
        let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;
        let priority_byte_offset =
            GICR_SGI_IPRIORITYR_BASE_OFFSET + u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID);

        assert_eq!(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID, 27);
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISPENDR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(base + priority_byte_offset, 0x40, 1)),
            MmioAction::WriteAccepted {
                value: 0x40,
                byte: 0x40,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + priority_byte_offset, 1)),
            MmioAction::ReadValue(0x40)
        );
        assert_eq!(
            gicr.acknowledge_pending_interrupt(0xff),
            GICV3_SPURIOUS_INTERRUPT_ID
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(gicr.acknowledge_pending_interrupt(0xff), 27);
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert!(gicr.end_interrupt(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID));
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert!(gicr.set_fdt_ppi_pending(WINDOWS_ARM_VIRTUAL_TIMER_PPI, true));
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ICPENDR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn gicv3_redistributor_selects_pending_ppi_by_priority_not_lowest_intid() {
        let mut gicr = GicV3RedistributorDevice::new(WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA);
        let base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;
        let other_ppi_interrupt_id = WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID + 1;
        let other_ppi_bit = 1_u32 << other_ppi_interrupt_id;
        let both_bits = timer_bit | other_ppi_bit;

        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(both_bits),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(both_bits),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(both_bits),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(both_bits),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_ISPENDR0_OFFSET,
                u64::from(both_bits),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(both_bits),
                byte: 0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IPRIORITYR_BASE_OFFSET
                    + u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
                0xa0,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0xa0,
                byte: 0xa0,
            }
        );
        assert_eq!(
            gicr.handle(MmioAccess::write(
                base + GICR_SGI_IPRIORITYR_BASE_OFFSET + u64::from(other_ppi_interrupt_id),
                0x20,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0x20,
                byte: 0x20,
            }
        );

        assert_eq!(
            gicr.pending_interrupt_id_for_cpu(0xff),
            Some(other_ppi_interrupt_id)
        );
        assert_eq!(
            gicr.acknowledge_pending_interrupt(0xff),
            other_ppi_interrupt_id
        );
        assert_eq!(
            gicr.handle(MmioAccess::read(base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(other_ppi_bit))
        );
        assert_eq!(
            gicr.pending_interrupt_id_for_cpu(0xff),
            Some(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID)
        );
    }

    #[test]
    fn firmware_system_register_trap_decoder_handles_gic_cpu_interface_regs() {
        let iar = decode_system_register_trap(sysreg_trap_syndrome(true, 2, 3, 0, 12, 12, 0))
            .expect("ICC_IAR1_EL1 trap decodes");
        assert!(iar.is_read);
        assert_eq!(iar.access_name(), "read");
        assert_eq!(iar.register, 2);
        assert_eq!(iar.sys_reg, ICC_IAR1_EL1_SYSREG);

        let eoir = decode_system_register_trap(sysreg_trap_syndrome(false, 4, 3, 0, 12, 12, 1))
            .expect("ICC_EOIR1_EL1 trap decodes");
        assert!(!eoir.is_read);
        assert_eq!(eoir.access_name(), "write");
        assert_eq!(eoir.register, 4);
        assert_eq!(eoir.sys_reg, ICC_EOIR1_EL1_SYSREG);

        assert_eq!(aarch64_sys_reg_encoding(3, 0, 4, 6, 0), ICC_PMR_EL1_SYSREG);
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 8, 4),
            ICC_AP0R0_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 9, 0),
            ICC_AP1R0_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 11, 3),
            ICC_RPR_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 12, 5),
            ICC_SRE_EL1_SYSREG
        );
        assert_eq!(
            aarch64_sys_reg_encoding(3, 0, 12, 12, 6),
            ICC_IGRPEN0_EL1_SYSREG
        );
        assert_eq!(decode_system_register_trap(0x93c0_8006), None);
    }

    #[test]
    fn gicv3_cpu_interface_accepts_group0_and_active_priority_registers() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();

        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_BPR0_EL1_SYSREG, 0x9),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_BPR0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(1))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN0_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IGRPEN0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(1))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                GICV3_SPURIOUS_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                GICV3_SPURIOUS_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_EOIR0_EL1_SYSREG, 0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_AP0R0_EL1_SYSREG, 0x1234),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_AP0R0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0x1234))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_AP1R0_EL1_SYSREG, 0x5678),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_AP1R0_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0x5678))
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_SGI1R_EL1_SYSREG, 0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
    }

    #[test]
    fn gicv3_cpu_interface_acknowledges_and_eois_pending_device_spis() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_ISENABLER_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_ISPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 0,
                    sys_reg: ICC_IGRPEN1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 7,
                },
                Some(1),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_IGROUPR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 0,
                    sys_reg: ICC_PMR_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 4,
                    crm: 6,
                    op2: 0,
                },
                Some(0xa0),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 0,
                    sys_reg: ICC_PMR_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 4,
                    crm: 6,
                    op2: 0,
                },
                Some(0xff),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: true,
                    register: 1,
                    sys_reg: ICC_HPPIR1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 2,
                },
                None,
            ),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: true,
                    register: 1,
                    sys_reg: ICC_IAR1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 0,
                },
                None,
            ),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xa0))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISPENDR_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            cpu.handle_system_register_access(
                &mut bus,
                DecodedSystemRegisterAccess {
                    is_read: false,
                    register: 1,
                    sys_reg: ICC_EOIR1_EL1_SYSREG,
                    op0: 3,
                    op1: 0,
                    crn: 12,
                    crm: 12,
                    op2: 1,
                },
                Some(34),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
    }

    #[test]
    fn gicv3_cpu_interface_acknowledges_and_eois_timer_ppi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xa0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xff),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_EOIR1_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn gicv3_cpu_interface_irq_line_snapshot_reports_timer_ppi_gates() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));

        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert!(!snapshot.group1_enabled);
        assert_eq!(snapshot.priority_mask, 0xff);
        assert_eq!(snapshot.running_priority, 0xff);
        assert_eq!(snapshot.priority_threshold, 0xff);
        assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
        assert!(!snapshot.irq_line_should_assert);

        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert!(snapshot.group1_enabled);
        assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
        assert!(!snapshot.irq_line_should_assert);

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert_eq!(
            snapshot.pending_intid,
            WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
        );
        assert!(snapshot.irq_line_should_assert);

        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_PMR_EL1_SYSREG, 0xa0),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        let snapshot = cpu.irq_line_snapshot(&mut bus);
        assert_eq!(snapshot.priority_mask, 0xa0);
        assert_eq!(snapshot.priority_threshold, 0xa0);
        assert_eq!(snapshot.pending_intid, GICV3_SPURIOUS_INTERRUPT_ID);
        assert!(!snapshot.irq_line_should_assert);
    }

    #[test]
    fn gicv3_cpu_interface_timer_ppi_does_not_clear_pending_spi_line() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicd_base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_IGROUPR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_ISENABLER_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_ISPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );

        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                GICV3_SPURIOUS_INTERRUPT_ID
            )))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_EOIR1_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert!(cpu.irq_line_should_assert(&mut bus));
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert!(!cpu.irq_line_should_assert(&mut bus));
    }

    #[test]
    fn gicv3_cpu_interface_selects_highest_priority_pending_across_ppi_and_spi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicd_base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        for register_base in [
            GICD_IGROUPR_BASE_OFFSET,
            GICD_ISENABLER_BASE_OFFSET,
            GICD_ISPENDR_BASE_OFFSET,
        ] {
            assert_eq!(
                bus.dispatch(MmioAccess::write(gicd_base + register_base + 4, 0x4, 4,)),
                MmioAction::WriteAccepted {
                    value: 0x4,
                    byte: 0x4,
                }
            );
        }
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicd_base + GICD_IPRIORITYR_BASE_OFFSET + 34,
                0x20,
                1,
            )),
            MmioAction::WriteAccepted {
                value: 0x20,
                byte: 0x20,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );

        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_HPPIR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0x20))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISPENDR0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
    }

    #[test]
    fn gicv3_cpu_interface_eoi_mode_requires_dir_to_deactivate_spi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let base = WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                base + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        for register_base in [
            GICD_IGROUPR_BASE_OFFSET,
            GICD_ISENABLER_BASE_OFFSET,
            GICD_ISPENDR_BASE_OFFSET,
        ] {
            assert_eq!(
                bus.dispatch(MmioAccess::write(base + register_base + 4, 0x4, 4)),
                MmioAction::WriteAccepted {
                    value: 0x4,
                    byte: 0x4,
                }
            );
        }
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_CTLR_EL1_SYSREG,
                ICC_CTLR_EL1_EOIMODE
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(34))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_EOIR1_EL1_SYSREG, 34),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_DIR_EL1_SYSREG, 34),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(base + GICD_ISACTIVER_BASE_OFFSET + 4, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn gicv3_cpu_interface_eoi_mode_requires_dir_to_deactivate_timer_ppi() {
        let block_devices = windows_arm_firmware_block_devices(None, None);
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        let mut cpu = GicV3CpuInterfaceState::new();
        let gicr_base = WINDOWS_ARM_GIC_REDISTRIBUTOR_MMIO_IPA;
        let timer_bit = 1_u32 << WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID;

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_IGROUPR0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                gicr_base + GICR_SGI_ISENABLER0_OFFSET,
                u64::from(timer_bit),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(timer_bit),
                byte: 0,
            }
        );
        assert!(set_windows_arm_firmware_vtimer_ppi_pending(&mut bus, true));
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_CTLR_EL1_SYSREG,
                ICC_CTLR_EL1_EOIMODE
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_write(&mut cpu, &mut bus, ICC_IGRPEN1_EL1_SYSREG, 1),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_IAR1_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(u64::from(
                WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID
            )))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_EOIR1_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: false,
            })
        );
        assert_eq!(
            gic_cpu_read(&mut cpu, &mut bus, ICC_RPR_EL1_SYSREG),
            Some(GicV3CpuInterfaceAction::Read(0xff))
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(u64::from(timer_bit))
        );
        assert_eq!(
            gic_cpu_write(
                &mut cpu,
                &mut bus,
                ICC_DIR_EL1_SYSREG,
                u64::from(WINDOWS_ARM_VIRTUAL_TIMER_INTERRUPT_ID),
            ),
            Some(GicV3CpuInterfaceAction::Write {
                refresh_level_sources: true,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicr_base + GICR_SGI_ISACTIVER0_OFFSET, 4)),
            MmioAction::ReadValue(0)
        );
    }

    #[test]
    fn firmware_block_queue_notify_selects_backing_by_mmio_ipa() {
        let stem = format!(
            "bridgevm-hvf-firmware-block-queue-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        );
        let iso_path = std::env::temp_dir().join(format!("{stem}.iso"));
        let disk_path = std::env::temp_dir().join(format!("{stem}.raw"));
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        let mut iso = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 32];
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            iso[sector_start + offset] = 0xc0_u8.wrapping_add(offset as u8);
            disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
        }
        std::fs::write(&iso_path, &iso).unwrap();
        std::fs::write(&disk_path, &disk).unwrap();

        let block_devices =
            windows_arm_firmware_block_devices(Some(iso_path.clone()), Some(disk_path.clone()));
        let mut bus = windows_arm_firmware_mmio_bus_with_block_devices(&block_devices);
        assert_eq!(block_devices[0].capacity_sectors, 16);
        assert_eq!(block_devices[1].capacity_sectors, 32);
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                4,
            )),
            MmioAction::ReadValue(16)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                4,
            )),
            MmioAction::ReadValue(32)
        );

        let installer_notify_ipa =
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
        let mut installer_backing = vec![0_u8; 16 * 1024];
        let mut installer_memory =
            VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut installer_backing);
        configure_virtio_block_queue_on_bus(&mut bus, WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA);
        seed_synthetic_virtio_block_read_request(&mut installer_memory).unwrap();
        let installer_status_ipa =
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_STATUS_OFFSET;
        let gicd_spi_pending_clear_ipa =
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ICPENDR_BASE_OFFSET + 4;
        let gicd_spi_pending_set_ipa =
            WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4;
        assert!(windows_arm_firmware_block_irq_source_may_change(
            &block_devices,
            installer_status_ipa,
            0,
        ));
        assert!(!windows_arm_firmware_block_irq_source_may_change(
            &block_devices,
            installer_status_ipa,
            VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE,
        ));
        assert!(
            windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                gicd_spi_pending_clear_ipa,
                0x4,
                4,
            )
        );
        assert!(
            !windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                gicd_spi_pending_set_ipa,
                0x4,
                4,
            )
        );
        assert!(
            !windows_arm_firmware_gicd_pending_clear_may_need_source_refresh(
                gicd_spi_pending_clear_ipa,
                0,
                4,
            )
        );
        assert_eq!(
            complete_windows_arm_firmware_block_queue_notify(
                &mut bus,
                &mut installer_memory,
                &block_devices,
                installer_status_ipa,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            ),
            Err(VirtioBlockRequestError::UnexpectedQueueNotifyIpa {
                role: "installer-iso",
                ipa: installer_status_ipa,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(installer_notify_ipa, 1, 4)),
            MmioAction::WriteAccepted { value: 1, byte: 1 }
        );
        assert_eq!(
            complete_windows_arm_firmware_block_queue_notify(
                &mut bus,
                &mut installer_memory,
                &block_devices,
                installer_notify_ipa,
                1,
            ),
            Err(VirtioBlockRequestError::UnsupportedQueueNotifyValue {
                role: "installer-iso",
                value: 1,
            })
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                installer_notify_ipa,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
            }
        );
        let installer_completion = complete_windows_arm_firmware_block_queue_notify(
            &mut bus,
            &mut installer_memory,
            &block_devices,
            installer_notify_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        )
        .unwrap();
        assert_eq!(installer_completion.role, "installer-iso");
        assert_eq!(installer_completion.backing_kind, "host-iso-readonly");
        assert_eq!(
            installer_completion.base_ipa,
            WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA
        );
        assert_eq!(
            installer_completion.completion.request_type,
            VIRTIO_BLK_T_IN
        );
        assert_eq!(installer_completion.completion.status, VIRTIO_BLK_S_OK);
        assert_eq!(installer_completion.byte_offset, 0xe00);
        assert_eq!(installer_completion.used_len, 513);
        assert_eq!(
            installer_memory
                .read_bytes(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_ADDRESS, 8)
                .unwrap(),
            vec![0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7]
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4,
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_CTLR_OFFSET,
                u64::from(GICD_CTLR_ENABLE_GRP1NS),
                4,
            )),
            MmioAction::WriteAccepted {
                value: u64::from(GICD_CTLR_ENABLE_GRP1NS),
                byte: GICD_CTLR_ENABLE_GRP1NS as u8,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IGROUPR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISENABLER_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::write(gicd_spi_pending_clear_ipa, 0x4, 4)),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(!windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert!(refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(gicd_spi_pending_set_ipa, 4)),
            MmioAction::ReadValue(0x4)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_VIRTIO_INSTALLER_ISO_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET,
                VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                byte: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE as u8,
            }
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ICPENDR_BASE_OFFSET + 4,
                0x4,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x4,
                byte: 0x4,
            }
        );
        assert!(!windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));

        let target_notify_ipa =
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET;
        let mut target_backing = vec![0_u8; 16 * 1024];
        let mut target_memory =
            VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut target_backing);
        configure_virtio_block_queue_on_bus(&mut bus, WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA);
        seed_synthetic_virtio_block_write_request_as_first(&mut target_memory).unwrap();
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                target_notify_ipa,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
            }
        );
        let target_completion = complete_windows_arm_firmware_block_queue_notify(
            &mut bus,
            &mut target_memory,
            &block_devices,
            target_notify_ipa,
            VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
        )
        .unwrap();
        assert_eq!(target_completion.role, "target-disk");
        assert_eq!(target_completion.backing_kind, "host-file-writable");
        assert_eq!(
            target_completion.base_ipa,
            WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA
        );
        assert_eq!(target_completion.completion.request_type, VIRTIO_BLK_T_OUT);
        assert_eq!(target_completion.completion.status, VIRTIO_BLK_S_OK);
        assert_eq!(target_completion.byte_offset, 0xe00);
        assert_eq!(target_completion.used_len, VIRTIO_BLOCK_STATUS_BYTES);
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4,
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0x8)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_IGROUPR_BASE_OFFSET + 4,
                0xc,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0xc,
                byte: 0xc,
            }
        );
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISENABLER_BASE_OFFSET + 4,
                0x8,
                4,
            )),
            MmioAction::WriteAccepted {
                value: 0x8,
                byte: 0x8,
            }
        );
        assert!(windows_arm_firmware_device_irq_line_assertable(
            &mut bus,
            &block_devices
        ));
        assert!(refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                WINDOWS_ARM_VIRTIO_TARGET_DISK_MMIO_IPA + VIRTIO_MMIO_INTERRUPT_ACK_OFFSET,
                VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
                byte: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE as u8,
            }
        );
        assert!(!refresh_windows_arm_firmware_device_irq_pending(
            &mut bus,
            &block_devices
        ));
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                WINDOWS_ARM_GIC_DISTRIBUTOR_MMIO_IPA + GICD_ISPENDR_BASE_OFFSET + 4,
                4,
            )),
            MmioAction::ReadValue(0)
        );
        let persisted = std::fs::read(&disk_path).unwrap();
        assert_eq!(
            &persisted[sector_start..sector_start + 8],
            &[0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
        );

        let _ = std::fs::remove_file(&iso_path);
        let _ = std::fs::remove_file(&disk_path);
    }

    #[test]
    fn mmio_serial_device_probe_render_records_three_exit_device_loop() {
        let probe = HvfMmioSerialDeviceProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            write_value_register_set: true,
            data_address_register_set: true,
            status_address_register_set: true,
            device_bus_created: true,
            device_bus_device_count: 1,
            write_run_attempted: true,
            write_exit_observed: true,
            write_handled_by_device: true,
            write_value_captured: true,
            pc_advanced_after_write: true,
            status_run_attempted: true,
            status_exit_observed: true,
            status_handled_by_device: true,
            status_value_injected: true,
            pc_advanced_after_status: true,
            continuation_run_attempted: true,
            continuation_exit_observed: true,
            status_value_preserved: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            device_model: PL011_UART_MODEL,
            code_ipa_start: 0x4000_0000,
            data_ipa: 0x5000_0000,
            status_ipa: 0x5000_0018,
            bytes: 16 * 1024,
            instructions: "STR X0, [X1]; LDR X0, [X2]; HVC #0",
            serial_write_value: 0x41,
            serial_status_value: 0x90,
            captured_write_value: Some(0x41),
            captured_byte: Some(0x41),
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            write_value_register_set_status: Some(0),
            data_address_register_set_status: Some(0),
            status_address_register_set_status: Some(0),
            write_run_status: Some(0),
            write_exit_reason: Some(1),
            write_exit_syndrome: Some(0x93c0_8046),
            write_exit_virtual_address: Some(0x5000_0000),
            write_exit_physical_address: Some(0x5000_0000),
            write_watchdog_cancel_status: None,
            write_value_capture_status: Some(0),
            pc_read_after_write_status: Some(0),
            pc_after_write_exit: Some(0x4000_0000),
            pc_advance_after_write_status: Some(0),
            status_run_status: Some(0),
            status_exit_reason: Some(1),
            status_exit_syndrome: Some(0x93c0_8006),
            status_exit_virtual_address: Some(0x5000_0018),
            status_exit_physical_address: Some(0x5000_0018),
            status_watchdog_cancel_status: None,
            status_value_set_status: Some(0),
            pc_read_after_status_status: Some(0),
            pc_after_status_exit: Some(0x4000_0004),
            pc_advance_after_status_status: Some(0),
            continuation_run_status: Some(0),
            continuation_exit_reason: Some(1),
            continuation_exit_syndrome: Some(0x5a00_0000),
            continuation_exit_virtual_address: Some(0),
            continuation_exit_physical_address: Some(0),
            continuation_watchdog_cancel_status: None,
            status_value_after_continue_status: Some(0),
            status_value_after_continue: Some(0x90),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Device model: PL011 UART skeleton"));
        assert!(output.contains("Device bus created: true"));
        assert!(output.contains("Device bus device count: 1"));
        assert!(output.contains("Write exit observed: true"));
        assert!(output.contains("Write handled by device: true"));
        assert!(output.contains("Write value captured: true"));
        assert!(output.contains("PC advanced after write: true"));
        assert!(output.contains("Status exit observed: true"));
        assert!(output.contains("Status handled by device: true"));
        assert!(output.contains("Status value injected: true"));
        assert!(output.contains("PC advanced after status: true"));
        assert!(output.contains("Continuation exit observed: true"));
        assert!(output.contains("Status value preserved: true"));
        assert!(output.contains("Captured write value: 0x41"));
        assert!(output.contains("Captured byte: 0x41"));
        assert!(output.contains("Write exit syndrome: 0x93c08046"));
        assert!(output.contains("Write exit virtual address: 0x50000000"));
        assert!(output.contains("Status exit syndrome: 0x93c08006"));
        assert!(output.contains("Status exit virtual address: 0x50000018"));
        assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
        assert!(output.contains("Status value after continue: 0x90"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_bus_routes_probe_serial_data_write() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

        assert_eq!(bus.device_count(), 1);
        assert_eq!(
            bus.dispatch(MmioAccess::write(0x5000_0000, 0x141, 8)),
            MmioAction::WriteAccepted {
                value: 0x141,
                byte: 0x41,
            }
        );
    }

    #[test]
    fn mmio_bus_routes_probe_serial_status_read() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

        assert_eq!(
            bus.dispatch(MmioAccess::read(0x5000_0018, 8)),
            MmioAction::ReadValue(0x90)
        );
    }

    #[test]
    fn mmio_bus_routes_pl031_rtc_read_after_uart_window() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));
        bus.attach(Box::new(Pl031RtcDevice::new(0x5000_1000, 0x2026_0618)));

        assert_eq!(bus.device_count(), 2);
        assert_eq!(
            bus.dispatch(MmioAccess::read(0x5000_1000, 8)),
            MmioAction::ReadValue(0x2026_0618)
        );
    }

    #[test]
    fn mmio_bus_routes_virtio_block_identity_registers_after_boot_devices() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        let block = VirtioMmioBlockDevice::new(0x5000_2000);
        let magic_ipa = block_base + VIRTIO_MMIO_MAGIC_VALUE_OFFSET;
        let version_ipa = block_base + VIRTIO_MMIO_VERSION_OFFSET;
        let device_id_ipa = block_base + VIRTIO_MMIO_DEVICE_ID_OFFSET;
        let vendor_id_ipa = block_base + VIRTIO_MMIO_VENDOR_ID_OFFSET;

        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));
        bus.attach(Box::new(Pl031RtcDevice::new(0x5000_1000, 0x2026_0618)));
        bus.attach(Box::new(block));

        assert_eq!(bus.device_count(), 3);
        assert_eq!(
            bus.dispatch(MmioAccess::read(magic_ipa, 4)),
            MmioAction::ReadValue(0x7472_6976)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(version_ipa, 4)),
            MmioAction::ReadValue(2)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(device_id_ipa, 4)),
            MmioAction::ReadValue(2)
        );
        assert_eq!(
            bus.dispatch(MmioAccess::read(vendor_id_ipa, 4)),
            MmioAction::ReadValue(0x4252_564d)
        );
    }

    #[test]
    fn mmio_bus_typed_lookup_skips_overlapping_wrong_type() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(Pl011UartDevice::new(block_base, 0x90)));
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));

        assert!(bus
            .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
            .is_some());
    }

    #[test]
    fn mmio_bus_routes_virtio_block_queue_and_config_registers() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));

        let writes = [
            (
                VIRTIO_MMIO_DRIVER_FEATURES_OFFSET,
                VIRTIO_MMIO_BLOCK_DRIVER_FEATURES_VALUE,
            ),
            (
                VIRTIO_MMIO_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_STATUS_ACK_VALUE,
            ),
            (
                VIRTIO_MMIO_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_STATUS_DRIVER_VALUE,
            ),
            (
                VIRTIO_MMIO_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_STATUS_FEATURES_OK_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_SEL_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_SEL_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_NUM_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
            (VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            (
                VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
            ),
        ];

        assert_eq!(
            bus.dispatch(MmioAccess::read(
                block_base + VIRTIO_MMIO_DEVICE_FEATURES_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_BLOCK_DEVICE_FEATURES_VALUE)
        );

        for (offset, value) in writes {
            assert_eq!(
                bus.dispatch(MmioAccess::write(block_base + offset, value, 4)),
                MmioAction::WriteAccepted {
                    value,
                    byte: (value & 0xff) as u8,
                }
            );
        }

        let reads = [
            (
                VIRTIO_MMIO_QUEUE_NUM_MAX_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_MAX_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
            ),
            (VIRTIO_MMIO_STATUS_OFFSET, VIRTIO_MMIO_BLOCK_STATUS_VALUE),
            (
                VIRTIO_MMIO_CONFIG_GENERATION_OFFSET,
                VIRTIO_MMIO_BLOCK_CONFIG_GENERATION_VALUE,
            ),
            (
                VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_BLOCK_CAPACITY_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS >> 32,
            ),
        ];

        for (offset, expected) in reads {
            assert_eq!(
                bus.dispatch(MmioAccess::read(block_base + offset, 4)),
                MmioAction::ReadValue(expected)
            );
        }
    }

    #[test]
    fn virtio_block_status_zero_resets_queue_state() {
        let mut bus = MmioBus::default();
        let block_base = 0x5000_2000;
        bus.attach(Box::new(VirtioMmioBlockDevice::new(block_base)));
        let mut backing = vec![0_u8; 16 * 1024];
        let mut memory = VirtioGuestMemory::new(WINDOWS_ARM_GUEST_RAM_IPA, &mut backing);

        configure_virtio_block_queue_on_bus(&mut bus, block_base);
        seed_synthetic_virtio_block_read_request(&mut memory).unwrap();
        assert_eq!(
            bus.dispatch(MmioAccess::write(
                block_base + VIRTIO_MMIO_QUEUE_NOTIFY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                4,
            )),
            MmioAction::WriteAccepted {
                value: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE,
                byte: VIRTIO_MMIO_BLOCK_QUEUE_NOTIFY_VALUE as u8,
            }
        );
        {
            let block = bus
                .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
                .unwrap();
            block
                .complete_next_available_block_request(&mut memory)
                .unwrap();
        }
        assert_eq!(
            bus.dispatch(MmioAccess::read(
                block_base + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4,
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );

        assert_eq!(
            bus.dispatch(MmioAccess::write(
                block_base + VIRTIO_MMIO_STATUS_OFFSET,
                0,
                4,
            )),
            MmioAction::WriteAccepted { value: 0, byte: 0 }
        );
        for (offset, expected) in [
            (VIRTIO_MMIO_STATUS_OFFSET, 0),
            (VIRTIO_MMIO_DRIVER_FEATURES_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_NUM_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_READY_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET, 0),
            (VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET, 0),
            (
                VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                VIRTIO_MMIO_BLOCK_INTERRUPT_STATUS_VALUE,
            ),
            (
                VIRTIO_MMIO_BLOCK_CAPACITY_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_CAPACITY_SECTORS & 0xffff_ffff,
            ),
        ] {
            assert_eq!(
                bus.dispatch(MmioAccess::read(block_base + offset, 4)),
                MmioAction::ReadValue(expected)
            );
        }

        configure_virtio_block_queue_on_bus(&mut bus, block_base);
        seed_synthetic_virtio_block_read_request(&mut memory).unwrap();
        let block = bus
            .find_device_mut_at::<VirtioMmioBlockDevice>(block_base)
            .unwrap();
        assert!(block
            .complete_next_available_block_request(&mut memory)
            .is_ok());
    }

    #[test]
    fn virtio_block_completes_one_available_read_request() {
        let block_base = 0x5000_2000;
        let guest_base = 0x4000_0000;
        let header_ipa = guest_base + 0x80;
        let data_ipa = guest_base + 0x400;
        let status_ipa = guest_base + 0x700;
        let sector = 7;
        let mut backing = vec![0_u8; 16 * 1024];
        let mut memory = VirtioGuestMemory::new(guest_base, &mut backing);
        let mut block = VirtioMmioBlockDevice::new(block_base);

        for (offset, value) in [
            (
                VIRTIO_MMIO_QUEUE_NUM_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_NUM_VALUE,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DESC_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DRIVER_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_LOW_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS & 0xffff_ffff,
            ),
            (
                VIRTIO_MMIO_QUEUE_DEVICE_HIGH_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS >> 32,
            ),
            (
                VIRTIO_MMIO_QUEUE_READY_OFFSET,
                VIRTIO_MMIO_BLOCK_QUEUE_READY_VALUE,
            ),
        ] {
            assert!(matches!(
                block.handle(MmioAccess::write(block_base + offset, value, 4)),
                MmioAction::WriteAccepted { .. }
            ));
        }

        memory.write_u32(header_ipa, VIRTIO_BLK_T_IN).unwrap();
        memory.write_u32(header_ipa + 4, 0).unwrap();
        memory.write_u64(header_ipa + 8, sector).unwrap();
        VirtqDescriptor {
            addr: header_ipa,
            len: VIRTIO_BLOCK_REQUEST_HEADER_BYTES,
            flags: VIRTQ_DESC_F_NEXT,
            next: 1,
        }
        .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 0)
        .unwrap();
        VirtqDescriptor {
            addr: data_ipa,
            len: 512,
            flags: VIRTQ_DESC_F_NEXT | VIRTQ_DESC_F_WRITE,
            next: 2,
        }
        .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 1)
        .unwrap();
        VirtqDescriptor {
            addr: status_ipa,
            len: VIRTIO_BLOCK_STATUS_BYTES,
            flags: VIRTQ_DESC_F_WRITE,
            next: 0,
        }
        .write(&mut memory, VIRTIO_MMIO_BLOCK_QUEUE_DESC_ADDRESS, 2)
        .unwrap();
        memory
            .write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 2, 1)
            .unwrap();
        memory
            .write_u16(VIRTIO_MMIO_BLOCK_QUEUE_DRIVER_ADDRESS + 4, 0)
            .unwrap();

        let completion = block
            .complete_next_available_block_request(&mut memory)
            .unwrap();

        assert_eq!(
            completion,
            VirtioBlockRequestCompletion {
                descriptor_index: 0,
                request_type: VIRTIO_BLK_T_IN,
                sector,
                data_bytes: 512,
                status: VIRTIO_BLK_S_OK,
                used_index: 1,
                interrupt_status: VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE,
            }
        );
        assert_eq!(block.completed_requests, 1);
        assert_eq!(
            memory.read_bytes(data_ipa, 8).unwrap(),
            (0..8)
                .map(|offset| synthetic_block_byte(sector, offset))
                .collect::<Vec<_>>()
        );
        assert_eq!(
            memory.read_bytes(status_ipa, 1).unwrap(),
            vec![VIRTIO_BLK_S_OK]
        );
        assert_eq!(
            memory
                .read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 4)
                .unwrap(),
            0
        );
        assert_eq!(
            memory
                .read_u32(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 8)
                .unwrap(),
            513
        );
        assert_eq!(
            memory
                .read_u16(VIRTIO_MMIO_BLOCK_QUEUE_DEVICE_ADDRESS + 2)
                .unwrap(),
            1
        );
        assert_eq!(
            block.handle(MmioAccess::read(
                block_base + VIRTIO_MMIO_INTERRUPT_STATUS_OFFSET,
                4
            )),
            MmioAction::ReadValue(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert_eq!(
            block.complete_next_available_block_request(&mut memory),
            Err(VirtioBlockRequestError::NoAvailableRequest)
        );
    }

    #[test]
    fn virtio_block_request_model_probe_reports_completion() {
        let probe = probe_virtio_block_request_model();
        let output = probe.render_text();

        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert!(probe.completed_via_device_bus);
        assert!(probe.completed);
        assert_eq!(probe.descriptor_index, Some(0));
        assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
        assert_eq!(probe.sector, Some(7));
        assert_eq!(probe.data_bytes, Some(512));
        assert_eq!(
            probe.data_prefix,
            (0..8)
                .map(|offset| synthetic_block_byte(7, offset))
                .collect::<Vec<_>>()
        );
        assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.used_index, Some(1));
        assert_eq!(probe.used_len, Some(513));
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block request model probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output
            .contains("Guest execution: not entered; in-memory VirtIO block descriptor chain"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Queue notify value: 0x0"));
        assert!(output.contains("Completed via device bus: true"));
        assert!(output.contains("Completed: true"));
        assert!(output.contains("Descriptor index: 0x0"));
        assert!(output.contains("Request type: 0x0"));
        assert!(output.contains("Sector: 0x7"));
        assert!(output.contains("Data bytes: 0x200"));
        assert!(output.contains("Data prefix: 0x0708090a0b0c0d0e"));
        assert!(output.contains("Status byte: 0x0"));
        assert!(output.contains("Used index: 0x1"));
        assert!(output.contains("Used length: 0x201"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn virtio_block_file_backing_probe_reads_from_host_file() {
        let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
        }
        let path = std::env::temp_dir().join(format!(
            "bridgevm-hvf-file-backed-{}-{}.img",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &disk).unwrap();

        let probe = probe_virtio_block_file_backing(path.clone());
        let output = probe.render_text();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.disk_path, path);
        assert_eq!(probe.backing_kind, "host-file");
        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert!(probe.completed_via_device_bus);
        assert!(probe.completed);
        assert_eq!(probe.descriptor_index, Some(0));
        assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
        assert_eq!(probe.sector, Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR));
        assert_eq!(probe.byte_offset, Some(0xe00));
        assert_eq!(
            probe.data_bytes,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
        );
        assert_eq!(
            probe.data_prefix,
            vec![0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7]
        );
        assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.used_index, Some(1));
        assert_eq!(probe.used_len, Some(513));
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block file backing probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains(
            "Guest execution: not entered; host file-backed VirtIO block descriptor chain"
        ));
        assert!(output.contains("Backing kind: host-file"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Completed via device bus: true"));
        assert!(output.contains("Completed: true"));
        assert!(output.contains("Descriptor index: 0x0"));
        assert!(output.contains("Request type: 0x0"));
        assert!(output.contains("Sector: 0x7"));
        assert!(output.contains("Byte offset: 0xe00"));
        assert!(output.contains("Data bytes: 0x200"));
        assert!(output.contains("Data prefix: 0xa0a1a2a3a4a5a6a7"));
        assert!(output.contains("Status byte: 0x0"));
        assert!(output.contains("Used index: 0x1"));
        assert!(output.contains("Used length: 0x201"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn virtio_block_writable_file_backing_probe_writes_flushes_and_persists() {
        let mut disk = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            disk[sector_start + offset] = 0xa0_u8.wrapping_add(offset as u8);
        }
        let path = std::env::temp_dir().join(format!(
            "bridgevm-hvf-writable-file-backed-{}-{}.img",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &disk).unwrap();

        let probe = probe_virtio_block_writable_file_backing(path.clone());
        let output = probe.render_text();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.disk_path, path);
        assert_eq!(probe.backing_kind, "host-file-writable");
        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert_eq!(
            probe.initial_read_prefix,
            vec![0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0xa6, 0xa7]
        );
        assert!(probe.write_completed);
        assert_eq!(probe.write_request_type, Some(VIRTIO_BLK_T_OUT));
        assert_eq!(
            probe.write_sector,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR)
        );
        assert_eq!(probe.write_byte_offset, Some(0xe00));
        assert_eq!(
            probe.write_data_bytes,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
        );
        assert_eq!(
            probe.write_data_prefix,
            vec![0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
        );
        assert_eq!(probe.write_status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.write_used_index, Some(2));
        assert_eq!(probe.write_used_len, Some(VIRTIO_BLOCK_STATUS_BYTES));
        assert!(probe.flush_completed);
        assert_eq!(probe.flush_request_type, Some(VIRTIO_BLK_T_FLUSH));
        assert_eq!(probe.flush_status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.flush_used_index, Some(3));
        assert_eq!(probe.flush_used_len, Some(VIRTIO_BLOCK_STATUS_BYTES));
        assert_eq!(
            probe.persisted_data_prefix,
            vec![0xe0, 0xe1, 0xe2, 0xe3, 0xe4, 0xe5, 0xe6, 0xe7]
        );
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block writable file backing probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains(
            "Guest execution: not entered; host file-backed VirtIO block write/flush persistence descriptor chain"
        ));
        assert!(output.contains("Backing kind: host-file-writable"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Initial read data prefix: 0xa0a1a2a3a4a5a6a7"));
        assert!(output.contains("Write completed: true"));
        assert!(output.contains("Write request type: 0x1"));
        assert!(output.contains("Write sector: 0x7"));
        assert!(output.contains("Write byte offset: 0xe00"));
        assert!(output.contains("Write data bytes: 0x200"));
        assert!(output.contains("Write data prefix: 0xe0e1e2e3e4e5e6e7"));
        assert!(output.contains("Write status byte: 0x0"));
        assert!(output.contains("Write used index: 0x2"));
        assert!(output.contains("Write used length: 0x1"));
        assert!(output.contains("Flush completed: true"));
        assert!(output.contains("Flush request type: 0x4"));
        assert!(output.contains("Flush status byte: 0x0"));
        assert!(output.contains("Flush used index: 0x3"));
        assert!(output.contains("Flush used length: 0x1"));
        assert!(output.contains("Persisted data prefix: 0xe0e1e2e3e4e5e6e7"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn virtio_block_iso_backing_probe_reads_from_read_only_iso() {
        let mut iso = vec![0_u8; (VIRTIO_BLOCK_SECTOR_BYTES as usize) * 16];
        let sector_start =
            (VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR * VIRTIO_BLOCK_SECTOR_BYTES) as usize;
        for offset in 0..VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES as usize {
            iso[sector_start + offset] = 0xc0_u8.wrapping_add(offset as u8);
        }
        let path = std::env::temp_dir().join(format!(
            "bridgevm-hvf-iso-backed-{}-{}.iso",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, &iso).unwrap();

        let probe = probe_virtio_block_iso_backing(path.clone());
        let output = probe.render_text();
        let _ = std::fs::remove_file(&path);

        assert_eq!(probe.iso_path, path);
        assert_eq!(probe.backing_kind, "host-iso-readonly");
        assert_eq!(probe.media_mode, "read-only");
        assert!(probe.configured_via_mmio);
        assert!(probe.configured_via_mmio_bus);
        assert!(probe.queue_notified);
        assert_eq!(probe.queue_notify_value, Some(0));
        assert!(probe.completed_via_device_bus);
        assert!(probe.completed);
        assert_eq!(probe.descriptor_index, Some(0));
        assert_eq!(probe.request_type, Some(VIRTIO_BLK_T_IN));
        assert_eq!(probe.sector, Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_SECTOR));
        assert_eq!(probe.byte_offset, Some(0xe00));
        assert_eq!(
            probe.data_bytes,
            Some(VIRTIO_BLOCK_SYNTHETIC_REQUEST_DATA_BYTES)
        );
        assert_eq!(
            probe.data_prefix,
            vec![0xc0, 0xc1, 0xc2, 0xc3, 0xc4, 0xc5, 0xc6, 0xc7]
        );
        assert_eq!(probe.status, Some(VIRTIO_BLK_S_OK));
        assert_eq!(probe.used_index, Some(1));
        assert_eq!(probe.used_len, Some(513));
        assert_eq!(
            probe.interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.readonly_write_rejected);
        assert_eq!(probe.readonly_write_status, Some(VIRTIO_BLK_S_IOERR));
        assert_eq!(probe.readonly_write_used_index, Some(2));
        assert_eq!(
            probe.readonly_write_used_len,
            Some(VIRTIO_BLOCK_STATUS_BYTES)
        );
        assert_eq!(
            probe.readonly_write_interrupt_status,
            Some(VIRTIO_MMIO_INTERRUPT_USED_BUFFER_VALUE)
        );
        assert!(probe.blockers.is_empty());
        assert!(output.contains("VirtIO block ISO backing probe"));
        assert!(output.contains("QEMU: not used"));
        assert!(output.contains("Apple VZ: not used"));
        assert!(output.contains("HVF: not entered"));
        assert!(output.contains(
            "Guest execution: not entered; read-only ISO-backed VirtIO block descriptor chain"
        ));
        assert!(output.contains(&format!("ISO path: {}", path.display())));
        assert!(output.contains("Backing kind: host-iso-readonly"));
        assert!(output.contains("Media mode: read-only"));
        assert!(output.contains("Configured via MMIO: true"));
        assert!(output.contains("Configured via MMIO bus: true"));
        assert!(output.contains("Queue notified: true"));
        assert!(output.contains("Completed via device bus: true"));
        assert!(output.contains("Completed: true"));
        assert!(output.contains("Descriptor index: 0x0"));
        assert!(output.contains("Request type: 0x0"));
        assert!(output.contains("Sector: 0x7"));
        assert!(output.contains("Byte offset: 0xe00"));
        assert!(output.contains("Data bytes: 0x200"));
        assert!(output.contains("Data prefix: 0xc0c1c2c3c4c5c6c7"));
        assert!(output.contains("Status byte: 0x0"));
        assert!(output.contains("Used index: 0x1"));
        assert!(output.contains("Used length: 0x201"));
        assert!(output.contains("Interrupt status: 0x1"));
        assert!(output.contains("Read-only write rejected: true"));
        assert!(output.contains("Read-only write status byte: 0x1"));
        assert!(output.contains("Read-only write used index: 0x2"));
        assert!(output.contains("Read-only write used length: 0x1"));
        assert!(output.contains("Read-only write interrupt status: 0x1"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains("qemu-system"));
        assert!(!output.contains('%'));
    }

    #[test]
    fn mmio_bus_reports_unmapped_access_as_unhandled() {
        let mut bus = MmioBus::default();
        bus.attach(Box::new(Pl011UartDevice::new(0x5000_0000, 0x90)));

        assert_eq!(
            bus.dispatch(MmioAccess::read(0x6000_0000, 8)),
            MmioAction::Unhandled
        );
    }

    #[test]
    fn mmio_rtc_device_probe_render_records_multi_device_continuation() {
        let probe = HvfMmioRtcDeviceProbe {
            allowed: true,
            attempted: true,
            vm_created: true,
            memory_allocated: true,
            memory_mapped: true,
            vcpu_created: true,
            pc_set: true,
            cpsr_set: true,
            rtc_address_register_set: true,
            device_bus_created: true,
            device_bus_device_count: 2,
            first_run_attempted: true,
            rtc_exit_observed: true,
            rtc_handled_by_device: true,
            rtc_value_injected: true,
            pc_read_after_rtc_exit: true,
            pc_advanced: true,
            second_run_attempted: true,
            continuation_exit_observed: true,
            rtc_value_preserved: true,
            watchdog_cancel_fired: false,
            vcpu_destroyed: true,
            memory_unmapped: true,
            vm_destroyed: true,
            memory_deallocated: true,
            host: HvfHostCapabilities {
                available: true,
                host: "macos-aarch64",
                default_ipa_bits: Some(36),
                max_ipa_bits: Some(40),
                el2_supported: Some(true),
                blockers: Vec::new(),
            },
            device_models: BOOT_MMIO_DEVICE_MODELS,
            code_ipa_start: 0x4000_0000,
            uart_ipa: 0x5000_0000,
            rtc_ipa: 0x5000_1000,
            bytes: 16 * 1024,
            instructions: "LDR X0, [X1]; HVC #0",
            rtc_value: 0x2026_0618,
            vm_create_status: Some(0),
            allocate_status: Some(0),
            map_status: Some(0),
            vcpu_create_status: Some(0),
            pc_set_status: Some(0),
            cpsr_set_status: Some(0),
            rtc_address_register_set_status: Some(0),
            first_run_status: Some(0),
            rtc_exit_reason: Some(1),
            rtc_exit_syndrome: Some(0x93c0_8006),
            rtc_exit_virtual_address: Some(0x5000_1000),
            rtc_exit_physical_address: Some(0x5000_1000),
            first_watchdog_cancel_status: None,
            rtc_value_set_status: Some(0),
            pc_read_status: Some(0),
            pc_after_rtc_exit: Some(0x4000_0000),
            pc_advance_status: Some(0),
            second_run_status: Some(0),
            continuation_exit_reason: Some(1),
            continuation_exit_syndrome: Some(0x5a00_0000),
            continuation_exit_virtual_address: Some(0),
            continuation_exit_physical_address: Some(0),
            second_watchdog_cancel_status: None,
            rtc_value_after_continue_status: Some(0),
            rtc_value_after_continue: Some(0x2026_0618),
            vcpu_destroy_status: Some(0),
            unmap_status: Some(0),
            vm_destroy_status: Some(0),
            deallocate_status: Some(0),
            blockers: Vec::new(),
        };
        let output = probe.render_text();

        assert!(output.contains("Allowed: true"));
        assert!(output.contains("Attempted: true"));
        assert!(output.contains("Device models: PL011 UART skeleton; PL031 RTC skeleton"));
        assert!(output.contains("Device bus created: true"));
        assert!(output.contains("Device bus device count: 2"));
        assert!(output.contains("RTC exit observed: true"));
        assert!(output.contains("RTC handled by device: true"));
        assert!(output.contains("RTC value injected: true"));
        assert!(output.contains("PC advanced: true"));
        assert!(output.contains("Continuation exit observed: true"));
        assert!(output.contains("RTC value preserved: true"));
        assert!(output.contains("RTC exit syndrome: 0x93c08006"));
        assert!(output.contains("RTC exit virtual address: 0x50001000"));
        assert!(output.contains("Continuation exit syndrome: 0x5a000000"));
        assert!(output.contains("RTC value after continue: 0x20260618"));
        assert!(output.contains("Blockers: none"));
        assert!(!output.contains('%'));
    }
}
