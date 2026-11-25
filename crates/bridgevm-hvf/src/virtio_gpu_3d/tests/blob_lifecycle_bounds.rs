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
fn reset_removes_guest_blob_mapping_before_releasing_renderer_backing() {
    const RESOURCE: u32 = 5;

    struct CheckingMapPort {
        backend: Arc<Mutex<MockBackend>>,
        port: Arc<Mutex<MockMapPort>>,
    }

    impl GpuShmMapPort for CheckingMapPort {
        fn map(&mut self, host_ptr: *mut u8, size: usize, offset: u64) -> Result<(), i32> {
            self.port.map(host_ptr, size, offset)
        }

        fn unmap(&mut self, offset: u64, size: usize) -> Result<(), i32> {
            let backend = self.backend.lock().unwrap();
            assert!(backend.mapped.contains_key(&RESOURCE));
            assert!(backend.unmapped.is_empty());
            self.port.unmap(offset, size)
        }
    }

    #[repr(align(16384))]
    struct Backing([u8; 0x4000]);
    let mut backing = Backing([0; 0x4000]);
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    backend.lock().unwrap().mapped.insert(
        RESOURCE,
        MappedBlob {
            host_ptr: backing.0.as_mut_ptr(),
            size: backing.0.len(),
            map_info: 0,
        },
    );
    let port = Arc::new(Mutex::new(MockMapPort::default()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    gpu.set_shm_map_port(
        Box::new(CheckingMapPort {
            backend: backend.clone(),
            port: port.clone(),
        }),
        0x10000,
    );
    let mut context = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, CTX);
    context.resize(CTX_CREATE_LEN, 0);
    ok(&mut gpu, &context);
    create_blob(&mut gpu, RESOURCE);
    let request = map_blob_req(RESOURCE, 0x4000);
    let hdr = CtrlHdr3d::parse(&request).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&request, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_MAP_INFO)
    );

    gpu.reset();

    assert_eq!(port.lock().unwrap().unmaps, [(0x4000, 0x4000)]);
    let backend = backend.lock().unwrap();
    assert_eq!(backend.unmapped, [RESOURCE]);
    assert!(backend.mapped.is_empty());
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

#[test]
fn host3d_4k_mappings_allow_adjacent_blobs_and_keep_overlap_checks() {
    #[repr(align(16384))]
    struct Backing([u8; 0x8000]);
    let mut backing = Backing([0; 0x8000]);
    let first = backing.0.as_mut_ptr();
    let subpage = first.wrapping_add(0x5000);
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let port = Arc::new(Mutex::new(MockMapPort {
        page_size: GpuShmPageSize::KiB4,
        ..MockMapPort::default()
    }));
    for (id, ptr, size) in [(5, first, 0x2100), (6, subpage, 0x1000)] {
        backend.lock().unwrap().mapped.insert(
            id,
            MappedBlob {
                host_ptr: ptr,
                size,
                map_info: 0,
            },
        );
    }

    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend));
    gpu.set_shm_map_port(Box::new(port.clone()), 0x10000);
    let mut context = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, CTX);
    context.resize(CTX_CREATE_LEN, 0);
    ok(&mut gpu, &context);
    ok(
        &mut gpu,
        &create_blob_req(5, VIRTIO_GPU_BLOB_MEM_HOST3D, 1, 0x2100, CTX),
    );
    ok(
        &mut gpu,
        &create_blob_req(6, VIRTIO_GPU_BLOB_MEM_HOST3D, 2, 0x1000, CTX),
    );

    for (id, offset, expected) in [
        (5, 0, VIRTIO_GPU_RESP_OK_MAP_INFO),
        (6, 0x2000, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER),
        (6, 0x3000, VIRTIO_GPU_RESP_OK_MAP_INFO),
    ] {
        let request = map_blob_req(id, offset);
        let hdr = CtrlHdr3d::parse(&request).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&request, hdr).unwrap(), 0),
            Some(expected)
        );
    }
    assert_eq!(
        port.lock().unwrap().maps,
        vec![
            (first as usize, 0x3000, 0),
            (subpage as usize, 0x1000, 0x3000)
        ]
    );
    ok(&mut gpu, &unmap_blob_req(5));
    ok(&mut gpu, &unmap_blob_req(6));
    assert_eq!(
        port.lock().unwrap().unmaps,
        vec![(0, 0x3000), (0x3000, 0x1000)]
    );
}
