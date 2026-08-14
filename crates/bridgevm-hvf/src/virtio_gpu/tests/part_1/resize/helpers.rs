//! Resize test helpers.

use super::super::*;

pub(super) fn install_2d_resource(
    dev: &mut VirtioPciGpu,
    resource_id: u32,
    width: u32,
    height: u32,
) {
    dev.gpu.resources.insert(
        resource_id,
        GpuResource {
            format: FORMAT_B8G8R8A8_UNORM,
            width,
            height,
            host_pixels: vec![0; scanout_len(width, height)],
            backing: Vec::new(),
        },
    );
}

pub(super) fn set_2d_scanout(dev: &mut VirtioPciGpu, resource_id: u32, width: u32, height: u32) {
    let mut request = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    push_rect(
        &mut request,
        Rect {
            x: 0,
            y: 0,
            width,
            height,
        },
    );
    request.extend_from_slice(&0u32.to_le_bytes());
    request.extend_from_slice(&resource_id.to_le_bytes());
    let mut response = Vec::new();
    dev.gpu.set_scanout_into(&request, None, &mut response);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
}
