//! virtio-net wire constants: register offsets, feature bits, queue indices, header limits, default MAC.

pub(crate) const MAGIC_VALUE: u32 = 0x7472_6976;

pub(crate) const VERSION_MODERN: u32 = 2;

pub(crate) const DEVICE_ID_NET: u32 = 1;

pub(crate) const VENDOR_ID_QEMU: u32 = 0x554d_4551;

pub(crate) const REG_MAGIC: u64 = 0x000;

pub(crate) const REG_VERSION: u64 = 0x004;

pub(crate) const REG_DEVICE_ID: u64 = 0x008;

pub(crate) const REG_VENDOR_ID: u64 = 0x00c;

pub(crate) const REG_DEVICE_FEATURES: u64 = 0x010;

pub(crate) const REG_DEVICE_FEATURES_SEL: u64 = 0x014;

pub(crate) const REG_DRIVER_FEATURES: u64 = 0x020;

pub(crate) const REG_DRIVER_FEATURES_SEL: u64 = 0x024;

pub(crate) const REG_QUEUE_SEL: u64 = 0x030;

pub(crate) const REG_QUEUE_NUM_MAX: u64 = 0x034;

pub(crate) const REG_QUEUE_NUM: u64 = 0x038;

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

pub(crate) const PCI_COMMON_CFG_OFFSET: u64 = 0x0000;

pub(crate) const PCI_ISR_CFG_OFFSET: u64 = 0x1000;

pub(crate) const PCI_DEVICE_CFG_OFFSET: u64 = 0x2000;

pub(crate) const PCI_NOTIFY_CFG_OFFSET: u64 = 0x3000;

pub(crate) const PCI_CFG_REGION_SIZE: u64 = 0x1000;

pub(crate) const COMMON_DEVICE_FEATURE_SELECT: u64 = 0x00;

pub(crate) const COMMON_DEVICE_FEATURE: u64 = 0x04;

pub(crate) const COMMON_DRIVER_FEATURE_SELECT: u64 = 0x08;

pub(crate) const COMMON_DRIVER_FEATURE: u64 = 0x0c;

pub(crate) const COMMON_CONFIG_MSIX_VECTOR: u64 = 0x10;

pub(crate) const COMMON_NUM_QUEUES: u64 = 0x12;

pub(crate) const COMMON_DEVICE_STATUS: u64 = 0x14;

pub(crate) const COMMON_CONFIG_GENERATION: u64 = 0x15;

pub(crate) const COMMON_QUEUE_SELECT: u64 = 0x16;

pub(crate) const COMMON_QUEUE_SIZE: u64 = 0x18;

pub(crate) const COMMON_QUEUE_MSIX_VECTOR: u64 = 0x1a;

pub(crate) const COMMON_QUEUE_ENABLE: u64 = 0x1c;

pub(crate) const COMMON_QUEUE_NOTIFY_OFF: u64 = 0x1e;

pub(crate) const COMMON_QUEUE_DESC: u64 = 0x20;

pub(crate) const COMMON_QUEUE_DRIVER: u64 = 0x28;

pub(crate) const COMMON_QUEUE_DEVICE: u64 = 0x30;

pub(crate) const VIRTIO_NET_F_MAC: u32 = 1 << 5;

pub(crate) const VIRTIO_NET_F_STATUS: u32 = 1 << 16;

pub(crate) const VIRTIO_F_VERSION_1: u32 = 1 << 0;

pub(crate) const VIRTIO_NET_S_LINK_UP: u16 = 1;

pub(crate) const VIRTIO_MSI_NO_VECTOR: u16 = 0xffff;

pub(crate) const QUEUE_RX: usize = 0;

pub(crate) const QUEUE_TX: usize = 1;

pub(crate) const QUEUE_COUNT: usize = 2;

pub(crate) const QUEUE_MAX: u16 = 256;

pub(crate) const DESC_SIZE: u64 = 16;

pub(crate) const DESC_F_NEXT: u16 = 1;

pub(crate) const DESC_F_WRITE: u16 = 2;

pub(crate) const VIRTIO_NET_HDR_LEN: usize = 12;

pub(crate) const MAX_TX_PACKET_LEN: usize = VIRTIO_NET_HDR_LEN + 65_535;

pub(crate) const DEFAULT_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x42, 0x56, 0x01];

pub(crate) fn offered_features_word(select: u32) -> u32 {
    match select {
        0 => VIRTIO_NET_F_MAC | VIRTIO_NET_F_STATUS,
        1 => VIRTIO_F_VERSION_1,
        _ => 0,
    }
}
