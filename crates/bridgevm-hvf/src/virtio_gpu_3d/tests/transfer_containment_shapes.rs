// Plain-buffer RESOURCE_CREATE_3D shapes used by the containment tests.

use super::super::*;
use super::helpers::*;

pub(super) fn create_constant_buffer(resource_id: u32) -> Vec<u8> {
    create_plain_buffer(resource_id, 64)
}

pub(super) fn create_plain_buffer(resource_id: u32, bind: u32) -> Vec<u8> {
    let mut request = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [resource_id, 0, 177, bind, 64, 1, 1, 1, 0, 0, 0, 0] {
        request.extend_from_slice(&field.to_le_bytes());
    }
    request
}
