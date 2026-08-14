//! Snapshot codec for requested and active display geometry.

use super::*;
use crate::checkpoint::{StateReader, StateWriter};

pub(super) fn write_geometry(out: &mut StateWriter, gpu: &VirtioGpu) {
    out.write_u32(gpu.width);
    out.write_u32(gpu.height);
    out.write_u32(gpu.requested_width);
    out.write_u32(gpu.requested_height);
}

pub(super) fn restore_geometry(input: &mut StateReader<'_>, gpu: &mut VirtioGpu, version: u32) {
    let width = input.read_u32();
    let height = input.read_u32();
    let (requested_width, requested_height) = if version >= 2 {
        (input.read_u32(), input.read_u32())
    } else {
        (width, height)
    };
    assert_eq!(
        (requested_width, requested_height),
        (gpu.requested_width, gpu.requested_height),
        "virtio-gpu resolution mismatch on restore"
    );
    gpu.width = width;
    gpu.height = height;
    gpu.requested_width = requested_width;
    gpu.requested_height = requested_height;
}
