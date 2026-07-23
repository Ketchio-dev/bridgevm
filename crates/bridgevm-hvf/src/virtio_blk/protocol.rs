//! virtio-blk wire constants: ids, register offsets, feature bits, request types and status codes.

use crate::machine;

pub const INSTALLER_ISO_SLOT: u64 = machine::VIRTIO_MMIO_COUNT - 1;

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976; // "virt"

pub(crate) const VERSION_LEGACY: u32 = 1;

pub(crate) const VERSION_MODERN: u32 = 2;

pub(crate) const DEVICE_ID_BLOCK: u32 = 2;

pub(crate) const VENDOR_ID_QEMU: u32 = 0x554d_4551; // "QEMU"

pub(crate) const REG_MAGIC: u64 = 0x000;

pub(crate) const REG_VERSION: u64 = 0x004;

pub(crate) const REG_DEVICE_ID: u64 = 0x008;

pub(crate) const REG_VENDOR_ID: u64 = 0x00c;

pub(crate) const REG_DEVICE_FEATURES: u64 = 0x010;

pub(crate) const REG_DEVICE_FEATURES_SEL: u64 = 0x014;

pub(crate) const REG_DRIVER_FEATURES: u64 = 0x020;

pub(crate) const REG_DRIVER_FEATURES_SEL: u64 = 0x024;

pub(crate) const REG_GUEST_PAGE_SIZE: u64 = 0x028;

pub(crate) const REG_QUEUE_SEL: u64 = 0x030;

pub(crate) const REG_QUEUE_NUM_MAX: u64 = 0x034;

pub(crate) const REG_QUEUE_NUM: u64 = 0x038;

pub(crate) const REG_QUEUE_ALIGN: u64 = 0x03c;

pub(crate) const REG_QUEUE_PFN: u64 = 0x040;

pub(crate) const REG_QUEUE_READY: u64 = 0x044;

pub(crate) const REG_QUEUE_NOTIFY: u64 = 0x050;

pub(crate) const REG_INTERRUPT_STATUS: u64 = 0x060;

pub(crate) const REG_INTERRUPT_ACK: u64 = 0x064;

pub(crate) const REG_STATUS: u64 = 0x070;

pub(crate) const REG_QUEUE_DESC_LOW: u64 = 0x080;

pub(crate) const REG_QUEUE_DESC_HIGH: u64 = 0x084;

pub(crate) const REG_QUEUE_DRIVER_LOW: u64 = 0x090;

pub(crate) const REG_QUEUE_DRIVER_HIGH: u64 = 0x094;

pub(crate) const REG_QUEUE_DEVICE_LOW: u64 = 0x0a0;

pub(crate) const REG_QUEUE_DEVICE_HIGH: u64 = 0x0a4;

pub(crate) const REG_CONFIG_GENERATION: u64 = 0x0fc;

pub(crate) const REG_CONFIG: u64 = 0x100;

pub(crate) const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;

pub(crate) const PCI_ISR_CFG_OFFSET: u64 = 0x1000;

pub(crate) const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;

pub(crate) const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;

pub(crate) const PCI_CFG_REGION_SIZE: u64 = 0x1000;

pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0; // bit 32, selected through features_sel=1

pub(crate) const VIRTIO_BLK_F_RO: u32 = 1 << 5;

pub(crate) const VIRTIO_BLK_F_BLK_SIZE: u32 = 1 << 6;

pub(crate) const QUEUE_MAX: u16 = 128;

pub(crate) const SECTOR_SIZE: u64 = 512;

pub(crate) const DESC_SIZE: u64 = 16;

pub(crate) const DESC_F_NEXT: u16 = 1;

pub(crate) const DESC_F_WRITE: u16 = 2;

pub(crate) const READ_CHUNK_BYTES: usize = 64 * 1024;

pub(crate) const VIRTIO_BLK_T_IN: u32 = 0;

pub(crate) const VIRTIO_BLK_S_OK: u8 = 0;

pub(crate) const VIRTIO_BLK_S_IOERR: u8 = 1;

pub(crate) const VIRTIO_BLK_S_UNSUPP: u8 = 2;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum VirtioMmioTransport {
    Legacy,
    Modern,
}

impl VirtioMmioTransport {
    pub(crate) const fn version(self) -> u32 {
        match self {
            Self::Legacy => VERSION_LEGACY,
            Self::Modern => VERSION_MODERN,
        }
    }
}
