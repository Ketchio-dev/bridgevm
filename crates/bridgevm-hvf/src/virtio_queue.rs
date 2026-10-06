//! Shared bounds helpers for virtio queue registers.

pub(crate) fn clamp_u16(value: u64, max: u16) -> u16 {
    value.min(u64::from(max)) as u16
}

#[cfg(test)]
#[path = "virtio_queue_overflow_tests.rs"]
mod overflow_tests;

#[path = "virtio_queue_address.rs"]
pub(crate) mod address;
