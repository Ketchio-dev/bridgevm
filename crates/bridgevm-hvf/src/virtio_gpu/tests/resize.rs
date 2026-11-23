//! Host-driven display resize regressions.

use super::*;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_GUEST;

// Blob scanout resize transitions.

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

// Resize test helpers.

fn install_2d_resource(dev: &mut VirtioPciGpu, resource_id: u32, width: u32, height: u32) {
    dev.gpu.resources.insert(
        resource_id,
        GpuResource {
            format: FORMAT_B8G8R8A8_UNORM,
            width,
            height,
            host_pixels: vec![0; scanout_len(width, height)],
            backing: Vec::new(),
            backing_len: 0,
        },
    );
}

fn set_2d_scanout(dev: &mut VirtioPciGpu, resource_id: u32, width: u32, height: u32) {
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

// 2D scanout resize transitions.

#[test]
fn host_resize_preserves_active_frame_until_guest_switches_modes() {
    let mut dev = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut dev, 1, 4, 3);
    dev.gpu.scanout_resource = Some(1);
    dev.gpu.scanout.fill(0x5a);
    let original = dev.gpu.scanout.clone();

    assert!(dev.request_display_resolution(6, 5));
    assert_eq!(dev.display_resolution(), (6, 5));
    let active = dev.gpu.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (4, 3, 16));
    assert_eq!(active.bytes, original);
    assert_eq!(dev.gpu.scanout_resource, Some(1));

    install_2d_resource(&mut dev, 2, 6, 5);
    set_2d_scanout(&mut dev, 2, 6, 5);

    let active = dev.gpu.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (6, 5, 24));
    assert_eq!(active.bytes, vec![0; 6 * 5 * 4]);
    assert_eq!(dev.gpu.scanout_resource, Some(2));
}

#[test]
fn nonmatching_scanout_does_not_commit_requested_geometry() {
    let mut dev = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut dev, 1, 4, 3);
    dev.gpu.scanout_resource = Some(1);
    dev.gpu.scanout.fill(0xa5);
    let original = dev.gpu.scanout.clone();

    assert!(dev.request_display_resolution(6, 5));
    install_2d_resource(&mut dev, 2, 5, 4);
    set_2d_scanout(&mut dev, 2, 5, 4);

    assert_eq!(dev.display_resolution(), (6, 5));
    let active = dev.gpu.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (4, 3, 16));
    assert_eq!(active.bytes, original);
}

// Active/requested geometry snapshot compatibility.

#[test]
fn snapshot_v2_round_trips_active_and_pending_geometry() {
    let mut source = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut source, 1, 4, 3);
    source.gpu.scanout_resource = Some(1);
    source.gpu.scanout.fill(0x3c);
    let resource = source.gpu.resources.get_mut(&1).unwrap();
    resource.backing = vec![
        BackingEntry {
            addr: 0x4000_8000,
            len: 12,
        },
        BackingEntry {
            addr: 0x4000_9000,
            len: 36,
        },
    ];
    resource.backing_len = 48;
    assert!(source.request_display_resolution(6, 5));
    let state = source.snapshot_state();

    let mut restored = VirtioPciGpu::new(4, 3);
    assert!(restored.request_display_resolution(6, 5));
    restored.restore_state(&state);

    assert_eq!(restored.display_resolution(), (6, 5));
    let active = restored.scanout().unwrap();
    assert_eq!((active.width, active.height, active.stride), (4, 3, 16));
    assert!(active.bytes.iter().all(|byte| *byte == 0x3c));
    assert_eq!(restored.gpu.resources[&1].backing_len, 48);
    assert_eq!(
        restored.gpu.resources[&1].backing,
        source.gpu.resources[&1].backing
    );
}

#[test]
fn snapshot_v1_defaults_requested_geometry_to_active_geometry() {
    let mut source = VirtioPciGpu::new(4, 3);
    install_2d_resource(&mut source, 1, 4, 3);
    source.gpu.scanout_resource = Some(1);
    source.gpu.scanout.fill(0x69);
    let v2 = source.snapshot_state();

    let mut v1 = Vec::with_capacity(v2.len() - 8);
    v1.extend_from_slice(&1u32.to_le_bytes());
    v1.extend_from_slice(&v2[4..12]);
    v1.extend_from_slice(&v2[20..]);

    let mut restored = VirtioPciGpu::new(4, 3);
    restored.restore_state(&v1);

    assert_eq!(restored.display_resolution(), (4, 3));
    let active = restored.scanout().unwrap();
    assert_eq!((active.width, active.height), (4, 3));
    assert!(active.bytes.iter().all(|byte| *byte == 0x69));
}
