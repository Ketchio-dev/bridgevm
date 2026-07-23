//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::fwcfg::GuestMemoryMut;
use std::alloc::alloc_zeroed;
use std::alloc::Layout;
use std::sync::Arc;
use std::sync::Mutex;

#[test]
fn host3d_blob_maps_through_shm_port_then_unmaps_before_unref() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let port = Arc::new(Mutex::new(MockMapPort::default()));
    let layout = Layout::from_size_align(0x1_0000, HVF_PAGE_SIZE as usize).unwrap();
    let ptr = unsafe { alloc_zeroed(layout) };
    assert!(!ptr.is_null());
    backend.lock().unwrap().mapped.insert(
        7,
        MappedBlob {
            host_ptr: ptr,
            size: 0x1_0000,
            map_info: 0x13,
        },
    );

    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    gpu.set_shm_map_port(Box::new(port.clone()), 0x20_0000);

    let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 0, 0x1_0000, 1);
    let hdr = CtrlHdr3d::parse(&create).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let map = map_blob_req(7, 0x4000);
    let hdr = CtrlHdr3d::parse(&map).unwrap();
    let response = gpu.handle(&map, hdr).unwrap();
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_MAP_INFO));
    assert_eq!(read_le_u32(&response, 24), Some(0x3));
    assert_eq!(
        port.lock().unwrap().maps,
        vec![(ptr as usize, 0x1_0000, 0x4000)]
    );

    let unmap = unmap_blob_req(7);
    let hdr = CtrlHdr3d::parse(&unmap).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&unmap, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(port.lock().unwrap().unmaps, vec![(0x4000, 0x1_0000)]);

    gpu.unref_resource(7);
    assert_eq!(backend.lock().unwrap().destroyed_resources, vec![7]);
}

#[test]
fn unmap_blob_rejects_classify_destroyed_and_unknown_resources() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let port = Arc::new(Mutex::new(MockMapPort::default()));
    let layout = Layout::from_size_align(0x1_0000, HVF_PAGE_SIZE as usize).unwrap();
    let ptr_a = unsafe { alloc_zeroed(layout) };
    let ptr_b = unsafe { alloc_zeroed(layout) };
    assert!(!ptr_a.is_null() && !ptr_b.is_null());
    for (resource_id, ptr) in [(7u32, ptr_a), (8u32, ptr_b)] {
        backend.lock().unwrap().mapped.insert(
            resource_id,
            MappedBlob {
                host_ptr: ptr,
                size: 0x1_0000,
                map_info: 0x13,
            },
        );
    }

    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    gpu.set_shm_map_port(Box::new(port.clone()), 0x20_0000);

    for (resource_id, offset) in [(7u32, 0x4000u64), (8, 0x1_8000)] {
        let create = create_blob_req(resource_id, VIRTIO_GPU_BLOB_MEM_HOST3D, 0, 0x1_0000, 1);
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        let map = map_blob_req(resource_id, offset);
        let hdr = CtrlHdr3d::parse(&map).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&map, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_MAP_INFO)
        );
    }

    // Blob 7 dies while mapped; blob 8 is unmapped first, then dies.
    gpu.unref_resource(7);
    let unmap = unmap_blob_req(8);
    let hdr = CtrlHdr3d::parse(&unmap).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&unmap, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    gpu.unref_resource(8);

    for (resource_id, expected) in [
        (7u32, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER),
        (8, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER),
        (99, VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER),
    ] {
        let unmap = unmap_blob_req(resource_id);
        let hdr = CtrlHdr3d::parse(&unmap).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&unmap, hdr).unwrap(), 0),
            Some(expected)
        );
    }
    let counts = gpu.unmap_blob_reject_counts();
    assert_eq!(counts.destroyed_while_mapped, 1);
    assert_eq!(counts.destroyed_after_unmap, 1);
    assert_eq!(counts.never_created, 1);
    assert_eq!(counts.short_request, 0);
    assert_eq!(counts.total(), 3);

    // Recreating an id starts a new lifecycle: the stale destroyed
    // classification must not survive into the reused id.
    backend.lock().unwrap().mapped.insert(
        7,
        MappedBlob {
            host_ptr: ptr_a,
            size: 0x1_0000,
            map_info: 0x13,
        },
    );
    let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 0, 0x1_0000, 1);
    let hdr = CtrlHdr3d::parse(&create).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    gpu.unref_resource(7);
    let unmap = unmap_blob_req(7);
    let hdr = CtrlHdr3d::parse(&unmap).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&unmap, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );
    let counts = gpu.unmap_blob_reject_counts();
    assert_eq!(counts.destroyed_while_mapped, 1);
    assert_eq!(counts.destroyed_after_unmap, 2);
    assert_eq!(counts.total(), 4);
}

#[test]
fn host3d_blob_map_rejects_zero_shm_window_without_shm_port() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));

    let create = create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 0, 0x4000, 1);
    let hdr = CtrlHdr3d::parse(&create).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let map = map_blob_req(7, 0);
    let hdr = CtrlHdr3d::parse(&map).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&map, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );
    assert!(backend.lock().unwrap().unmapped.is_empty());

    let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO, 0);
    info.extend_from_slice(&0u32.to_le_bytes());
    info.extend_from_slice(&0u32.to_le_bytes());
    let hdr = CtrlHdr3d::parse(&info).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&info, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_CAPSET_INFO)
    );
}

#[test]
fn get_capset_uses_backend_capset_into_without_cloning_capset_vec() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET, 0);
    get.extend_from_slice(&4u32.to_le_bytes());
    get.extend_from_slice(&1u32.to_le_bytes());
    let hdr = CtrlHdr3d::parse(&get).unwrap();

    let mut out = Vec::with_capacity(24 + 160);
    let response_ptr = out.as_ptr();
    assert!(gpu.handle_with_mem_into(None, &get, hdr, &mut out));

    assert_eq!(out.len(), 24 + 160);
    assert_eq!(read_le_u32(&out, 0), Some(VIRTIO_GPU_RESP_OK_CAPSET));
    assert_eq!(read_le_u32(&out, 24), Some(1));
    assert_eq!(out.as_ptr(), response_ptr);
    assert_eq!(backend.lock().unwrap().capset_calls, 0);
}

#[test]
fn invalid_capset_returns_one_header_instead_of_appending_it_to_ok_response() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend));
    let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET, 0);
    get.extend_from_slice(&99u32.to_le_bytes());
    get.extend_from_slice(&1u32.to_le_bytes());
    let hdr = CtrlHdr3d::parse(&get).unwrap();

    let response = gpu.handle(&get, hdr).unwrap();
    assert_eq!(response.len(), 24);
    assert_eq!(
        read_le_u32(&response, 0),
        Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );
}

#[test]
fn drain_completed_fences_into_reuses_caller_storage_and_counts() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let sentinel = CompletedFence {
        ctx_id: 99,
        ring_idx: 0,
        fence_id: 1,
    };
    let completed = [
        CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        },
        CompletedFence {
            ctx_id: 4,
            ring_idx: 5,
            fence_id: 6,
        },
    ];
    backend.lock().unwrap().completed.extend(completed);

    let mut out = Vec::with_capacity(4);
    out.push(sentinel);
    let out_ptr = out.as_ptr();
    let out_capacity = out.capacity();

    gpu.drain_completed_fences_into(&mut out);

    assert_eq!(out.as_ptr(), out_ptr);
    assert_eq!(out.capacity(), out_capacity);
    assert_eq!(out, vec![sentinel, completed[0], completed[1]]);
    assert_eq!(gpu.stats(0).fences_completed, 2);
    assert!(backend.lock().unwrap().completed.is_empty());
    assert_eq!(backend.lock().unwrap().fence_polls, 1);
    assert_eq!(backend.lock().unwrap().fence_after_queue_polls, 0);

    gpu.drain_completed_fences_after_queue_into(&mut out);
    assert_eq!(backend.lock().unwrap().fence_polls, 1);
    assert_eq!(backend.lock().unwrap().fence_after_queue_polls, 1);

    let wrapper_fence = CompletedFence {
        ctx_id: 7,
        ring_idx: 8,
        fence_id: 9,
    };
    backend.lock().unwrap().completed.push(wrapper_fence);
    assert_eq!(gpu.drain_completed_fences(), vec![wrapper_fence]);
    assert_eq!(gpu.stats(0).fences_completed, 3);
}

#[test]
fn ctx_attach_detach_blob_resource_without_live_context_forwards_to_backend() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));

    let create = create_blob_req(11, VIRTIO_GPU_BLOB_MEM_HOST3D, 44, 0x4000, 9);
    let hdr = CtrlHdr3d::parse(&create).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let attach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 27, 11);
    let hdr = CtrlHdr3d::parse(&attach).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&attach, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let detach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_DETACH_RESOURCE, 27, 11);
    let hdr = CtrlHdr3d::parse(&detach).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&detach, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let backend = backend.lock().unwrap();
    assert_eq!(backend.attached, vec![(27, 11)]);
    assert_eq!(backend.detached, vec![(27, 11)]);
}

#[test]
fn ctx_attach_unknown_resource_errors_without_forwarding() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));

    let attach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 27, 99);
    let hdr = CtrlHdr3d::parse(&attach).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&attach, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );

    assert!(backend.lock().unwrap().attached.is_empty());
}

#[test]
fn ctx_attach_registered_2d_resource_forwards_to_backend() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    gpu.register_2d_resource(5);

    let attach = ctx_resource_req(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 31, 5);
    let hdr = CtrlHdr3d::parse(&attach).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&attach, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert_eq!(backend.lock().unwrap().attached, vec![(31, 5)]);
}

#[test]
fn guest_blob_create_forwards_resolved_iovecs_to_backend() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mem = TestMem::new(0x8000_0000, 0x20_000);

    let create = create_blob_req_with_entries(
        19,
        VIRTIO_GPU_BLOB_MEM_GUEST,
        77,
        0x3000,
        3,
        &[
            BlobMemEntry {
                addr: 0x8000_1000,
                len: 0x1000,
            },
            BlobMemEntry {
                addr: 0x8000_4000,
                len: 0x2000,
            },
        ],
    );
    let hdr = CtrlHdr3d::parse(&create).unwrap();

    assert_eq!(
        read_le_u32(&gpu.handle_with_mem(Some(&mem), &create, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let backend = backend.lock().unwrap();
    assert_eq!(
        backend.blobs,
        vec![(19, VIRTIO_GPU_BLOB_MEM_GUEST, 77, 0x3000)]
    );
    assert_eq!(backend.blob_iovecs, vec![(19, 2, 0x3000)]);
}

#[test]
fn guest_blob_create_reuses_host_iovec_scratch() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mem = TestMem::new(0x8000_0000, 0x20_000);
    let entries = [
        BlobMemEntry {
            addr: 0x8000_1000,
            len: 0x1000,
        },
        BlobMemEntry {
            addr: 0x8000_4000,
            len: 0x2000,
        },
    ];

    let mut previous_scratch = None;
    for resource_id in [29, 30] {
        let create = create_blob_req_with_entries(
            resource_id,
            VIRTIO_GPU_BLOB_MEM_GUEST,
            77,
            0x3000,
            3,
            &entries,
        );
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle_with_mem(Some(&mem), &create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
        assert!(gpu.host_iovecs_scratch.is_empty());
        let scratch = (
            gpu.host_iovecs_scratch.as_ptr() as usize,
            gpu.host_iovecs_scratch.capacity(),
        );
        assert!(scratch.1 >= entries.len());
        if let Some(previous) = previous_scratch {
            assert_eq!(scratch, previous);
        }
        previous_scratch = Some(scratch);
    }

    assert_eq!(
        backend.lock().unwrap().blob_iovecs,
        vec![(29, 2, 0x3000), (30, 2, 0x3000)]
    );
}

#[test]
fn legacy_virgl_resource_backing_and_bidirectional_transfers_reach_backend() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mem = TestMem::new(0x1000, 0x4000);

    let create_args = Create3dArgs {
        resource_id: 41,
        target: 2,
        format: 1,
        bind: 0x402,
        width: 640,
        height: 480,
        depth: 1,
        array_size: 1,
        last_level: 0,
        nr_samples: 0,
        flags: 0,
    };
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        create_args.resource_id,
        create_args.target,
        create_args.format,
        create_args.bind,
        create_args.width,
        create_args.height,
        create_args.depth,
        create_args.array_size,
        create_args.last_level,
        create_args.nr_samples,
        create_args.flags,
        0,
    ] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    let hdr = CtrlHdr3d::parse(&create).unwrap();
    let response = gpu.handle(&create, hdr).unwrap();
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert!(gpu.is_3d_resource(41));

    assert!(gpu.attach_3d_backing(
        &mem,
        41,
        &[
            BlobMemEntry {
                addr: 0x1000,
                len: 0x1000,
            },
            BlobMemEntry {
                addr: 0x2000,
                len: 0x2000,
            },
        ],
    ));

    for (typ, to_host) in [
        (VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, true),
        (VIRTIO_GPU_CMD_TRANSFER_FROM_HOST_3D, false),
    ] {
        let mut transfer = ctrl_req(typ, 7);
        for field in [3u32, 4, 0, 32, 16, 1] {
            transfer.extend_from_slice(&field.to_le_bytes());
        }
        transfer.extend_from_slice(&128u64.to_le_bytes());
        for field in [41u32, 2, 256, 4096] {
            transfer.extend_from_slice(&field.to_le_bytes());
        }
        let hdr = CtrlHdr3d::parse(&transfer).unwrap();
        let response = gpu.handle(&transfer, hdr).unwrap();
        assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
        assert_eq!(
            backend.lock().unwrap().transfers_3d.last().unwrap().1,
            to_host
        );
    }

    assert!(gpu.detach_3d_backing(41));
    gpu.unref_resource(41);
    let inner = backend.lock().unwrap();
    assert_eq!(inner.created_3d, vec![create_args]);
    assert_eq!(inner.backing_attached, vec![(41, 2, 0x3000)]);
    assert_eq!(inner.backing_detached, vec![41]);
    assert_eq!(inner.transfers_3d.len(), 2);
    assert_eq!(inner.transfers_3d[0].0.resource_id, 41);
    assert_eq!(inner.transfers_3d[0].0.ctx_id, 7);
    assert_eq!(inner.transfers_3d[0].0.width, 32);
    assert_eq!(inner.destroyed_resources, vec![41]);
}

#[test]
fn empty_context_zero_submit_is_an_immediate_noop() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mut submit = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, 0);
    submit.extend_from_slice(&0u32.to_le_bytes());
    submit.extend_from_slice(&0u32.to_le_bytes());
    let hdr = CtrlHdr3d::parse(&submit).unwrap();
    let response = gpu.handle(&submit, hdr).unwrap();
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert!(backend.lock().unwrap().submits.is_empty());
}

#[test]
fn pre_context_wddm_copy_region_updates_scattered_local_scanout_backing() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mut mem = TestMem::new(0x8000_0000, 0x1000);

    for resource_id in [1, 2] {
        let create = local_scanout_create_req(resource_id, 4, 3);
        let hdr = CtrlHdr3d::parse(&create).unwrap();
        assert_eq!(
            read_le_u32(&gpu.handle(&create, hdr).unwrap(), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
    }

    let dst_entries = [
        BlobMemEntry {
            addr: 0x8000_0100,
            len: 10,
        },
        BlobMemEntry {
            addr: 0x8000_0180,
            len: 38,
        },
    ];
    let src_entries = [
        BlobMemEntry {
            addr: 0x8000_0200,
            len: 13,
        },
        BlobMemEntry {
            addr: 0x8000_0280,
            len: 35,
        },
    ];
    assert!(gpu.attach_3d_backing(&mem, 1, &dst_entries));
    assert!(gpu.attach_3d_backing(&mem, 2, &src_entries));

    let src_pixels: Vec<u8> = (0..48).map(|value| value + 1).collect();
    assert!(mem.write_bytes(src_entries[0].addr, &src_pixels[..13]));
    assert!(mem.write_bytes(src_entries[1].addr, &src_pixels[13..]));

    let submit = resource_copy_submit_req(4, 1, 0, 0, 2, 1, 1, 2, 2);
    let hdr = CtrlHdr3d::parse(&submit).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle_with_mem(Some(&mem), &submit, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut dst_pixels = vec![0u8; 48];
    assert!(read_scattered_backing_into(
        &mem,
        &dst_entries,
        0,
        &mut dst_pixels
    ));
    assert_eq!(&dst_pixels[0..8], &src_pixels[20..28]);
    assert_eq!(&dst_pixels[16..24], &src_pixels[36..44]);
    assert!(dst_pixels[8..16].iter().all(|byte| *byte == 0));
    assert!(dst_pixels[24..].iter().all(|byte| *byte == 0));
    assert_eq!(gpu.local_copy_submits, 1);
    assert_eq!(gpu.stats(0).submits, 1);
    let backend = backend.lock().unwrap();
    assert!(backend.created_3d.is_empty());
    assert!(backend.submits.is_empty());
}

#[test]
fn pre_context_non_copy_submit_remains_rejected() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let mem = TestMem::new(0x8000_0000, 0x1000);
    let mut submit = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, 4);
    submit.extend_from_slice(&4u32.to_le_bytes());
    submit.extend_from_slice(&0u32.to_le_bytes());
    submit.extend_from_slice(&0x1234_5678u32.to_le_bytes());
    let hdr = CtrlHdr3d::parse(&submit).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle_with_mem(Some(&mem), &submit, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );
    assert_eq!(gpu.local_copy_submits, 0);
    assert!(backend.lock().unwrap().submits.is_empty());
}
