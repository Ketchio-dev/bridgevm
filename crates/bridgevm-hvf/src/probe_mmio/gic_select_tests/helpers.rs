//! Shared fixtures for these tests.

use crate::probe_mmio::*;

pub(super) fn configure_virtio_block_queue_on_bus(bus: &mut MmioBus, block_base: u64) {
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
