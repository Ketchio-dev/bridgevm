// Walk coverage: a rejection must still be found after earlier commands.

use super::super::*;
use super::helpers::*;
use super::transfer_containment::*;
use super::transfer_containment_shapes::*;
use std::sync::{Arc, Mutex};

#[test]
fn live_shaped_stream_rejects_the_second_unbacked_buffer() {
    // Reproduces run3 of t8 job 20260824-115428-72165-14984: a submit whose
    // FIRST TRANSFER3D targets a backed buffer (resource 11) and whose later
    // TRANSFER3D targets an attached-but-never-backed buffer (resource 153).
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let ctx_id = 7;
    handle(&mut gpu, &create_context(ctx_id));
    for resource_id in [11, 153] {
        handle(&mut gpu, &create_constant_buffer(resource_id));
        handle(
            &mut gpu,
            &ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, ctx_id, resource_id),
        );
    }
    let mem = TestMem::new(0x8000_0000, 0x1000);
    assert!(gpu.attach_3d_backing(
        &mem,
        11,
        &[BlobMemEntry {
            addr: 0x8000_0000,
            len: 64
        }]
    ));

    let mut two = transfer_submit(ctx_id, 11);
    let second = transfer_submit(ctx_id, 153);
    let payload = &second[SUBMIT_3D_LEN..];
    two.extend_from_slice(payload);
    let size = (two.len() - SUBMIT_3D_LEN) as u32;
    two[24..28].copy_from_slice(&size.to_le_bytes());

    let hdr = CtrlHdr3d::parse(&two).unwrap();
    let response = gpu.handle(&two, hdr).unwrap();
    assert_eq!(
        read_le_u32(&response, 0),
        Some(VIRTIO_GPU_RESP_ERR_UNSPEC),
        "unbacked resource 153 later in the stream must still be contained"
    );
    assert!(backend.lock().unwrap().submits.is_empty());
}

#[test]
fn an_unregistered_resource_does_not_abandon_the_rest_of_the_walk() {
    // Regression for a fail-open: an earlier TRANSFER3D naming a resource this
    // device never registered must not stop the walk before the unbacked one.
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let ctx_id = 7;
    handle(&mut gpu, &create_context(ctx_id));
    handle(&mut gpu, &create_constant_buffer(153));
    handle(
        &mut gpu,
        &ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, ctx_id, 153),
    );

    let mut two = transfer_submit(ctx_id, 9999);
    let second = transfer_submit(ctx_id, 153);
    two.extend_from_slice(&second[SUBMIT_3D_LEN..]);
    let size = (two.len() - SUBMIT_3D_LEN) as u32;
    two[24..28].copy_from_slice(&size.to_le_bytes());

    let hdr = CtrlHdr3d::parse(&two).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&two, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_UNSPEC)
    );
    assert!(backend.lock().unwrap().submits.is_empty());
}
