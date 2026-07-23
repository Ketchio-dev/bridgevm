//! Split out of probe_mmio.rs by responsibility.

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

#[cfg(test)]
#[path = "gic_select_tests/mod.rs"]
mod tests;
