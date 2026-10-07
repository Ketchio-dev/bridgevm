//! Guest-chosen blob ids and sizes cannot grow host bookkeeping or abort a map.

use super::super::resource_unref::MAX_DESTROYED_BLOB_IDS;
use super::super::*;
use super::helpers::*;
use std::sync::{Arc, Mutex};

const CTX: u32 = 7;

fn ok(gpu: &mut VirtioGpu3d, request: &[u8]) {
    let hdr = CtrlHdr3d::parse(request).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(request, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
}

fn gpu_with_context() -> VirtioGpu3d {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend));
    let mut create = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, CTX);
    create.resize(CTX_CREATE_LEN, 0);
    ok(&mut gpu, &create);
    gpu
}

fn create_blob(gpu: &mut VirtioGpu3d, resource_id: u32) {
    let blob_id = u64::from(resource_id);
    let request = create_blob_req(
        resource_id,
        VIRTIO_GPU_BLOB_MEM_HOST3D,
        blob_id,
        0x4000,
        CTX,
    );
    ok(gpu, &request);
}

fn attach(gpu: &mut VirtioGpu3d, resource_id: u32) {
    let request = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, CTX, resource_id);
    ok(gpu, &request);
}

#[test]
fn unref_detaches_the_resource_from_every_context() {
    let mut gpu = gpu_with_context();
    create_blob(&mut gpu, 5);
    attach(&mut gpu, 5);
    gpu.unref_resource(5);
    assert!(!gpu.ctx_has_resource(CTX, 5));

    // A reused id is attached only after the guest attaches it again.
    create_blob(&mut gpu, 5);
    assert!(!gpu.ctx_has_resource(CTX, 5));
    attach(&mut gpu, 5);
    assert!(gpu.ctx_has_resource(CTX, 5));
}

#[test]
fn resource_churn_keeps_context_and_destroyed_id_bookkeeping_bounded() {
    let mut gpu = gpu_with_context();
    let rounds = MAX_DESTROYED_BLOB_IDS as u32 + 512;
    for resource_id in 1..=rounds {
        create_blob(&mut gpu, resource_id);
        attach(&mut gpu, resource_id);
        gpu.unref_resource(resource_id);
    }
    assert!(gpu.blob_resources.is_empty());
    assert!(gpu.ctx_resources[&CTX].is_empty());
    assert_eq!(
        gpu.destroyed_blob_unmapped_ids.len(),
        MAX_DESTROYED_BLOB_IDS
    );
    // The latest destroyed id still classifies a late UNMAP_BLOB.
    assert!(gpu.destroyed_blob_unmapped_ids.contains(&rounds));
}

#[test]
fn map_blob_rejects_a_size_without_a_page_rounded_footprint() {
    let mut gpu = gpu_with_context();
    gpu.set_shm_map_port(
        Box::new(Arc::new(Mutex::new(MockMapPort::default()))),
        1 << 30,
    );
    for (resource_id, size) in [(5, u64::MAX), (6, u64::MAX - 0x3fff + 1)] {
        let request = create_blob_req(resource_id, VIRTIO_GPU_BLOB_MEM_HOST3D, 1, size, CTX);
        ok(&mut gpu, &request);
        let map = map_blob_req(resource_id, 0);
        let hdr = CtrlHdr3d::parse(&map).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&map, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
        );
    }
}
