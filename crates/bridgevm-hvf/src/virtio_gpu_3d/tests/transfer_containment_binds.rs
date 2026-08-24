// Binding must not gate containment: any unbacked PIPE_BUFFER poisons the ctx.

use super::super::*;
use super::helpers::*;
use super::transfer_containment::*;
use super::transfer_containment_shapes::*;
use std::sync::{Arc, Mutex};

#[test]
fn vertex_and_index_buffers_are_contained_like_constant_buffers() {
    // Batch-5 lanes did not all fail on a constant buffer: run 10's resource 305
    // was created bind=16 (VERTEX_BUFFER) and run 17's resource 303 bind=32
    // (INDEX_BUFFER). The renderer rejects any PIPE_BUFFER without an iov, so
    // the preflight must not key on the binding.
    for bind in [16u32, 32, 64] {
        let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
        let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
        let ctx_id = 7;
        let resource_id = 305;
        handle(&mut gpu, &create_context(ctx_id));
        handle(&mut gpu, &create_plain_buffer(resource_id, bind));
        handle(
            &mut gpu,
            &ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, ctx_id, resource_id),
        );

        let out = handle(&mut gpu, &transfer_submit(ctx_id, resource_id));
        assert_eq!(
            read_le_u32(&out, 0),
            Some(VIRTIO_GPU_RESP_ERR_UNSPEC),
            "bind {bind:#x} must be refused before the renderer"
        );
        let diagnostic = gpu.take_submit_diagnostic().expect("diagnostic recorded");
        assert_eq!(diagnostic.resource_id, Some(resource_id));
        assert_eq!(diagnostic.resource_backed, Some(false));
    }
}
