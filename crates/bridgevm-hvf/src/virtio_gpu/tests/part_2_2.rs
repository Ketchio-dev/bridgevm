//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_HOST3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DESTROY;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_CONTEXT_INIT;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_RESOURCE_BLOB;
use crate::virtio_gpu_3d::VIRTIO_GPU_F_VIRGL;
use std::time::Duration;

#[test]
fn resource_unref_unbinds_bound_host3d_blob_scanout() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let mut host_pixels = vec![0u8; 16];
    backend.lock().unwrap().mapped.insert(
        14,
        virtio_gpu_3d::MappedBlob {
            host_ptr: host_pixels.as_mut_ptr(),
            size: host_pixels.len(),
            map_info: 0,
        },
    );
    let create = create_blob_req(14, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
    let set_scanout = set_scanout_blob_req(14, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
    let mut unref = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_UNREF);
    unref.extend_from_slice(&14u32.to_le_bytes());
    unref.extend_from_slice(&0u32.to_le_bytes());

    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &unref, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert!(dev.scanout().is_none());
    assert_eq!(backend.lock().unwrap().unmapped, vec![14]);
    assert_eq!(backend.lock().unwrap().destroyed_resources, vec![14]);
}

#[test]
fn ctx_destroy_unbinds_attached_blob_scanout_before_teardown() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let mut host_pixels = vec![0u8; 16];
    backend.lock().unwrap().mapped.insert(
        15,
        virtio_gpu_3d::MappedBlob {
            host_ptr: host_pixels.as_mut_ptr(),
            size: host_pixels.len(),
            map_info: 0,
        },
    );
    let create_ctx = ctx_create_req(1, 4, b"ctx");
    let create_blob = create_blob_req(15, VIRTIO_GPU_BLOB_MEM_HOST3D, 16, &[]);
    let mut attach = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 1);
    attach.extend_from_slice(&15u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    let set_scanout = set_scanout_blob_req(15, 1, 1, FORMAT_B8G8R8A8_UNORM, 4, 0);
    let destroy = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_DESTROY, 1);

    for request in [&create_ctx, &create_blob, &attach, &set_scanout, &destroy] {
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, request, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
    }

    assert!(dev.scanout().is_none());
    assert_eq!(backend.lock().unwrap().unmapped, vec![15]);
    assert_eq!(backend.lock().unwrap().destroyed, vec![1]);
}

#[test]
fn unknown_command_returns_err_unspec_without_wedging() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let resp = submit_control(&mut dev, &mut mem, &ctrl_req(0xdead_beef), 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_ERR_UNSPEC));
    assert_eq!(dev.stats().queues[0].last_avail_idx, 1);
}

#[test]
fn three_d_backend_advertises_features_and_capsets() {
    let (mut dev, _) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(
            VIRTIO_GPU_F_EDID
                | VIRTIO_GPU_F_VIRGL
                | VIRTIO_GPU_F_RESOURCE_BLOB
                | VIRTIO_GPU_F_CONTEXT_INIT
        )
    );
    // num_scanouts @8 stays 1; num_capsets @12 is 1 with a backend.
    assert_eq!(
        pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 8, 4, &mut mem),
        1
    );
    assert_eq!(
        pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 12, 4, &mut mem),
        1
    );
    let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
    info.extend_from_slice(&0u32.to_le_bytes());
    info.extend_from_slice(&0u32.to_le_bytes());
    let resp = submit_control(&mut dev, &mut mem, &info, 40);
    assert_eq!(
        read_le_u32(&resp, 0),
        Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET_INFO)
    );
    assert_eq!(read_le_u32(&resp, 24), Some(4));
    assert_eq!(read_le_u32(&resp, 28), Some(1));
    assert_eq!(read_le_u32(&resp, 32), Some(160));
    assert!(dev.gpu.response_scratch.is_empty());

    let mut get = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET);
    get.extend_from_slice(&4u32.to_le_bytes());
    get.extend_from_slice(&1u32.to_le_bytes());
    let resp = submit_control(&mut dev, &mut mem, &get, 24 + 160);
    assert_eq!(
        read_le_u32(&resp, 0),
        Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET)
    );
    assert_eq!(read_le_u32(&resp, 24), Some(1));
    let response_capacity = dev.gpu.response_scratch.capacity();
    let response_ptr = dev.gpu.response_scratch.as_ptr();
    assert!(response_capacity >= resp.len());
    assert!(dev.gpu.response_scratch.is_empty());

    let resp = submit_control(&mut dev, &mut mem, &get, 24 + 160);
    assert_eq!(
        read_le_u32(&resp, 0),
        Some(virtio_gpu_3d::VIRTIO_GPU_RESP_OK_CAPSET)
    );
    assert_eq!(dev.gpu.response_scratch.capacity(), response_capacity);
    assert_eq!(dev.gpu.response_scratch.as_ptr(), response_ptr);
    assert!(dev.gpu.response_scratch.is_empty());
}

#[test]
fn legacy_virgl_commands_route_through_common_backing_and_control_queue() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);

    let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [31u32, 2, 1, 0x402, 320, 200, 1, 1, 0, 0, 0, 0] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut backing = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    backing.extend_from_slice(&31u32.to_le_bytes());
    backing.extend_from_slice(&1u32.to_le_bytes());
    backing.extend_from_slice(&0x4002_0000u64.to_le_bytes());
    backing.extend_from_slice(&0x1000u32.to_le_bytes());
    backing.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &backing, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert_eq!(
        read_le_u32(
            &submit_control(&mut dev, &mut mem, &ctx_create_req(7, 0, b""), 24),
            0
        ),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    let mut attach = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 7);
    attach.extend_from_slice(&31u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut transfer = ctrl_req_ctx(VIRTIO_GPU_CMD_TRANSFER_TO_HOST_3D, 7);
    for field in [0u32, 0, 0, 32, 16, 1] {
        transfer.extend_from_slice(&field.to_le_bytes());
    }
    transfer.extend_from_slice(&0u64.to_le_bytes());
    for field in [31u32, 0, 128, 2048] {
        transfer.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &transfer, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let inner = backend.lock().unwrap();
    assert_eq!(inner.created_3d.len(), 1);
    assert_eq!(inner.created_3d[0].resource_id, 31);
    assert_eq!(inner.backing_attached, vec![(31, 1, 0x1000)]);
    assert_eq!(inner.attached, vec![(7, 31)]);
    assert_eq!(inner.transfers_3d.len(), 1);
    assert!(inner.transfers_3d[0].1);
    assert_eq!(inner.transfers_3d[0].0.resource_id, 31);
}

#[test]
fn legacy_virgl_3d_resource_can_drive_cpu_scanout_on_flush() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);

    let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        31u32,
        2,
        FORMAT_B8G8R8A8_UNORM,
        0x8a,
        1920,
        1080,
        1,
        1,
        0,
        1,
        0,
        0,
    ] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    for field in [0u32, 0, 1280, 800, 0, 31] {
        set_scanout.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
    for field in [0u32, 0, 1280, 800, 31, 0] {
        flush.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);
    let stats = dev.stats();
    assert_eq!(stats.scanout_3d_flushes, 1);
    assert_eq!(stats.scanout_readback_attempts, 1);
    assert_eq!(stats.scanout_readbacks, 1);
    assert_eq!(stats.scanout_readback_throttled, 0);
    assert_eq!(stats.scanout_readback_bytes, 1280 * 800 * 4);
    let scanout = dev.gpu.scanout().expect("3D scanout should be active");
    assert_eq!(&scanout.bytes[..8], &[0, 1, 2, 3, 4, 5, 6, 7]);
}

#[test]
fn smaller_legacy_3d_scanout_uses_resource_dimensions_and_display_stride() {
    let (mut dev, backend) = dev_with_mock();
    dev.gpu = VirtioGpu::with_3d_backend(6, 4, Box::new(backend.clone()));
    let mut mem = TestMem::new(0x4000_0000, 0x30000);

    let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        31u32,
        2,
        FORMAT_B8G8R8A8_UNORM,
        0x8a,
        4,
        3,
        1,
        1,
        0,
        1,
        0,
        0,
    ] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    for field in [0u32, 0, 4, 3, 0, 31] {
        set_scanout.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
    for field in [0u32, 0, 4, 3, 31, 0] {
        flush.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 4, 3)]);
    let scanout = dev.gpu.scanout().expect("3D scanout should be active");
    assert_eq!(&scanout.bytes[..16], &(0u8..16).collect::<Vec<_>>());
    assert_eq!(&scanout.bytes[16..24], &[0; 8]);
    assert_eq!(&scanout.bytes[24..40], &(16u8..32).collect::<Vec<_>>());
    assert_eq!(&scanout.bytes[40..48], &[0; 8]);
    assert_eq!(&scanout.bytes[72..], &[0; 24]);
    let stats = dev.stats();
    assert_eq!(stats.scanout_readback_attempts, 1);
    assert_eq!(stats.scanout_readbacks, 1);
    assert_eq!(stats.scanout_readback_bytes, 4 * 3 * 4);
}

#[test]
fn venus_wddm_primary_uses_guest_backing_with_dual_renderer_backend() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x50_0000);

    let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        31u32,
        2,
        FORMAT_B8G8R8A8_UNORM,
        0x4008a,
        1024,
        768,
        1,
        1,
        0,
        0,
        0,
        0,
    ] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &create, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let mut ctx_attach = ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_ATTACH_RESOURCE, 3);
    ctx_attach.extend_from_slice(&31u32.to_le_bytes());
    ctx_attach.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &ctx_attach, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let backing_addr = 0x4010_0000u64;
    let backing_len = 1024u32 * 768 * 4;
    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&31u32.to_le_bytes());
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&backing_addr.to_le_bytes());
    attach.extend_from_slice(&backing_len.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    mem.write(backing_addr, &[1, 2, 3, 4, 5, 6, 7, 8]);

    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    for field in [0u32, 0, 1024, 768, 0, 31] {
        set_scanout.extend_from_slice(&field.to_le_bytes());
    }
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &set_scanout, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let flush = flush_req(
        31,
        Rect {
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        },
    );
    assert_eq!(
        read_le_u32(&submit_control(&mut dev, &mut mem, &flush, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    let inner = backend.lock().unwrap();
    assert!(inner.created_3d.is_empty());
    assert!(inner.attached.is_empty());
    assert!(inner.backing_attached.is_empty());
    assert!(inner.scanout_reads.is_empty());
    drop(inner);
    let scanout = dev
        .gpu
        .scanout()
        .expect("local 3D scanout should be active");
    assert_eq!(&scanout.bytes[..8], &[1, 2, 3, 0, 5, 6, 7, 0]);
    let stats = dev.stats();
    assert_eq!(stats.scanout_3d_flushes, 1);
    assert_eq!(stats.scanout_readbacks, 1);
    assert_eq!(stats.scanout_readback_bytes, 8);
}

#[test]
fn legacy_virgl_scanout_readback_can_be_throttled_to_display_pacing() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);

    let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
    for field in [
        31u32,
        2,
        FORMAT_B8G8R8A8_UNORM,
        0x8a,
        1280,
        800,
        1,
        1,
        0,
        1,
        0,
        0,
    ] {
        create.extend_from_slice(&field.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &create, 24);
    let mut set_scanout = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    for field in [0u32, 0, 1280, 800, 0, 31] {
        set_scanout.extend_from_slice(&field.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &set_scanout, 24);
    dev.gpu
        .set_3d_scanout_readback_interval(Duration::from_secs(60));

    let mut flush = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_FLUSH);
    for field in [0u32, 0, 1280, 800, 31, 0] {
        flush.extend_from_slice(&field.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &flush, 24);
    submit_control(&mut dev, &mut mem, &flush, 24);

    assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);
    let stats = dev.stats();
    assert_eq!(stats.scanout_3d_flushes, 2);
    assert_eq!(stats.scanout_readback_attempts, 1);
    assert_eq!(stats.scanout_readbacks, 1);
    assert_eq!(stats.scanout_readback_throttled, 1);
    assert_eq!(stats.scanout_readback_bytes, 1280 * 800 * 4);
}
