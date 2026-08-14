//! Blob scanout resize transitions.

use super::super::*;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_GUEST;

#[test]
fn matching_blob_scanout_commits_requested_geometry() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let backing = 0x4000_8000;
    let blob_len = 6 * 5 * 4;

    dev.gpu.scanout_resource = Some(1);
    assert!(dev.request_display_resolution(6, 5));
    let create = create_blob_req(
        7,
        VIRTIO_GPU_BLOB_MEM_GUEST,
        blob_len,
        &[(backing, blob_len as u32)],
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let set_scanout = set_scanout_blob_req(7, 6, 5, FORMAT_B8G8R8A8_UNORM, 24, 0);
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let active = dev.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (6, 5, 24));
    assert_eq!(dev.display_resolution(), (6, 5));
    assert!(dev.gpu.scanout_resource.is_none());
    assert_eq!(
        dev.gpu
            .blob_scanout
            .as_ref()
            .map(|scanout| (scanout.width, scanout.height)),
        Some((6, 5))
    );
}
