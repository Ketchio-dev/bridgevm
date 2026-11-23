//! virtio-net wire constants: device id, feature bits, queue indices, header limits, default MAC.

pub(crate) const DEVICE_ID_NET: u32 = 1;

pub(crate) const VIRTIO_NET_F_MAC: u32 = 1 << 5;

pub(crate) const VIRTIO_NET_F_STATUS: u32 = 1 << 16;

pub(crate) const VIRTIO_NET_S_LINK_UP: u16 = 1;

pub(crate) const QUEUE_RX: usize = 0;

pub(crate) const QUEUE_TX: usize = 1;

pub(crate) const QUEUE_COUNT: usize = 2;

pub(crate) const QUEUE_MAX: u16 = 256;

pub(crate) const VIRTIO_NET_HDR_LEN: usize = 12;

pub(crate) const MAX_TX_PACKET_LEN: usize = VIRTIO_NET_HDR_LEN + 65_535;

pub(crate) const DEFAULT_MAC: [u8; 6] = [0x52, 0x54, 0x00, 0x42, 0x56, 0x01];
