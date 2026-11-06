//! TRANSFER_TO_HOST_2D with a guest-chosen backing offset: an offset whose
//! row or pixel position overflows u64 completes with OK_NODATA and copies
//! nothing, and a non-origin box starts at the supplied offset.

use super::super::*;
use super::helpers::*;

fn transfer_fixture(width: u32, height: u32, bytes: &[u8]) -> (VirtioPciGpu, TestMem) {
    let mut dev = VirtioPciGpu::new(width, height);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let backing = 0x4000_8000u64;
    mem.write(backing, bytes);
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    for field in [1, FORMAT_B8G8R8A8_UNORM, width, height] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&backing.to_le_bytes());
    attach.extend_from_slice(&(bytes.len() as u32).to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    for request in [&create, &attach] {
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, request, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
    }
    (dev, mem)
}

fn transfer(dev: &mut VirtioPciGpu, mem: &mut TestMem, rect: Rect, offset: u64) {
    let mut request = ctrl_req(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_2D);
    push_rect(&mut request, rect);
    request.extend_from_slice(&offset.to_le_bytes());
    request.extend_from_slice(&1u32.to_le_bytes());
    request.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(dev, mem, &request, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
}

#[test]
fn transfer_offset_pixel_overflow_preserves_pixels_and_completes() {
    let (mut dev, mut mem) = transfer_fixture(2, 1, &[0xa5; 8]);
    transfer(
        &mut dev,
        &mut mem,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
        u64::MAX,
    );
    assert_eq!(dev.gpu.resources[&1].host_pixels, [0; 8]);
}

#[test]
fn transfer_offset_row_overflow_preserves_pixels_and_completes() {
    let (mut dev, mut mem) = transfer_fixture(1, 2, &[0xa5; 8]);
    transfer(
        &mut dev,
        &mut mem,
        Rect {
            x: 0,
            y: 0,
            width: 1,
            height: 2,
        },
        u64::MAX,
    );
    assert_eq!(dev.gpu.resources[&1].host_pixels, [0; 8]);
}

#[test]
fn transfer_offset_non_origin_box_starts_at_supplied_offset() {
    let backing: Vec<u8> = (0..16).collect();
    let (mut dev, mut mem) = transfer_fixture(3, 2, &backing);
    transfer(
        &mut dev,
        &mut mem,
        Rect {
            x: 1,
            y: 1,
            width: 2,
            height: 1,
        },
        4,
    );
    let mut expected = vec![0; 24];
    expected[16..24].copy_from_slice(&backing[4..12]);
    assert_eq!(dev.gpu.resources[&1].host_pixels, expected);
}
