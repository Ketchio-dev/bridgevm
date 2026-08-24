// The prefix before a rejected command must reach the renderer, as virgl does.

use super::super::*;
use super::helpers::*;
use super::transfer_containment::*;
use super::transfer_containment_shapes::*;
use std::sync::{Arc, Mutex};

#[test]
fn commands_before_the_rejected_transfer_still_reach_the_renderer() {
    // Batch 6 (job 20260824-154650-73552-15846) measured the cost of refusing a
    // whole buffer: every lane with a preflight rejection then reported
    // "Illegal handle" on the same context, because the discarded prefix -- 168
    // to 616 bytes -- carried CREATE_OBJECT commands the guest never repeats.
    // virglrenderer executes the prefix before it stops (vrend_decode.c:2160),
    // so containment must too.
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let ctx_id = 7;
    handle(&mut gpu, &create_context(ctx_id));
    handle(&mut gpu, &create_constant_buffer(153));
    handle(
        &mut gpu,
        &ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, ctx_id, 153),
    );

    // A three-dword CREATE_OBJECT-shaped command ahead of the bad transfer.
    let create_object: [u32; 3] = [1 | (2 << 16), 0x2a, 0x2a];
    let mut submit = transfer_submit(ctx_id, 153);
    let transfer = submit[SUBMIT_3D_LEN..].to_vec();
    submit.truncate(SUBMIT_3D_LEN);
    for dword in create_object {
        submit.extend_from_slice(&dword.to_le_bytes());
    }
    submit.extend_from_slice(&transfer);
    let size = (submit.len() - SUBMIT_3D_LEN) as u32;
    submit[24..28].copy_from_slice(&size.to_le_bytes());

    let hdr = CtrlHdr3d::parse(&submit).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&submit, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_UNSPEC),
        "the unbacked transfer is still refused"
    );
    let submits = backend.lock().unwrap().submits.clone();
    assert_eq!(submits.len(), 1, "the prefix must reach the renderer");
    assert_eq!(
        submits[0].1,
        create_object
            .iter()
            .flat_map(|d| d.to_le_bytes())
            .collect::<Vec<u8>>(),
        "exactly the commands before the rejected one, and nothing more"
    );
    assert_eq!(
        gpu.take_submit_diagnostic().and_then(|d| d.resource_id),
        Some(153)
    );
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
    let submits = backend.lock().unwrap().submits.clone();
    assert_eq!(
        submits.len(),
        1,
        "the unregistered-resource prefix still runs"
    );
    assert_eq!(submits[0].1.len(), SUBMIT_3D_PAYLOAD_DWORDS_14 * 4);
}
