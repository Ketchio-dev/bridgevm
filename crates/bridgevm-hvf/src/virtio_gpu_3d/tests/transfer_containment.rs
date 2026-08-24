// Unbacked TRANSFER3D containment and context survival.

use super::super::submit_backing_preflight::VIRGL_CCMD_TRANSFER3D;
use super::super::*;
use super::helpers::*;
use std::sync::{Arc, Mutex};

pub(super) fn handle(gpu: &mut VirtioGpu3d, request: &[u8]) -> Vec<u8> {
    let hdr = CtrlHdr3d::parse(request).unwrap();
    gpu.handle(request, hdr).unwrap()
}

pub(super) fn create_context(ctx_id: u32) -> Vec<u8> {
    let mut request = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, ctx_id);
    request.extend_from_slice(&4u32.to_le_bytes());
    request.extend_from_slice(&0u32.to_le_bytes());
    let mut name = [0u8; 64];
    name[..4].copy_from_slice(b"test");
    request.extend_from_slice(&name);
    request
}

pub(super) fn create_constant_buffer(resource_id: u32) -> Vec<u8> {
    let mut request = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [resource_id, 0, 177, 64, 64, 1, 1, 1, 0, 0, 0, 0] {
        request.extend_from_slice(&field.to_le_bytes());
    }
    request
}

pub(super) fn transfer_submit(ctx_id: u32, resource_id: u32) -> Vec<u8> {
    let mut command = Vec::new();
    for dword in [
        VIRGL_CCMD_TRANSFER3D | (13u32 << 16),
        resource_id,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        64,
        1,
        1,
        0,
        1,
    ] {
        command.extend_from_slice(&dword.to_le_bytes());
    }
    let mut request = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, ctx_id);
    request.extend_from_slice(&(command.len() as u32).to_le_bytes());
    request.extend_from_slice(&0u32.to_le_bytes());
    request.extend_from_slice(&command);
    request
}

#[test]
fn unbacked_transfer_is_contained_and_the_context_survives() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let (ctx_id, resource_id) = (7, 140);
    assert_eq!(
        read_le_u32(&handle(&mut gpu, &create_context(ctx_id)), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&handle(&mut gpu, &create_constant_buffer(resource_id)), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(
            &handle(
                &mut gpu,
                &ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, ctx_id, resource_id)
            ),
            0
        ),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let submit = transfer_submit(ctx_id, resource_id);
    assert_eq!(
        read_le_u32(&handle(&mut gpu, &submit), 0),
        Some(VIRTIO_GPU_RESP_ERR_UNSPEC)
    );
    assert!(backend.lock().unwrap().submits.is_empty());
    assert_eq!(
        gpu.take_submit_diagnostic(),
        Some(Submit3dDiagnostic {
            renderer_status: 22,
            command_offset_dwords: Some(0),
            command_id: Some(VIRGL_CCMD_TRANSFER3D),
            command_header: Some(VIRGL_CCMD_TRANSFER3D | (13u32 << 16)),
            resource_id: Some(resource_id),
            resource_found: Some(true),
            resource_backed: Some(false),
        })
    );

    let mem = TestMem::new(0x8000_0000, 0x1000);
    assert!(gpu.attach_3d_backing(
        &mem,
        resource_id,
        &[BlobMemEntry {
            addr: 0x8000_0000,
            len: 64
        }]
    ));
    assert_eq!(
        read_le_u32(&handle(&mut gpu, &submit), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(backend.lock().unwrap().submits.len(), 1);
    assert_eq!(gpu.take_submit_diagnostic(), None);

    assert!(gpu.detach_3d_backing(resource_id));
    assert_eq!(
        read_le_u32(&handle(&mut gpu, &submit), 0),
        Some(VIRTIO_GPU_RESP_ERR_UNSPEC)
    );
    assert_eq!(backend.lock().unwrap().submits.len(), 1);
}

#[test]
fn malformed_stream_is_left_to_the_renderer_without_panicking() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let gpu = VirtioGpu3d::with_backend(Box::new(backend));
    let truncated = (VIRGL_CCMD_TRANSFER3D | (13u32 << 16)).to_le_bytes();
    assert_eq!(gpu.preflight_unbacked_buffer_transfer(7, &truncated), None);
}
