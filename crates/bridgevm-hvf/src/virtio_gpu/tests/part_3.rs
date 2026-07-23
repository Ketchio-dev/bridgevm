//! Split test module.

use super::super::*;
use super::helpers::*;
use crate::virtio_gpu_3d;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_CTX_DESTROY;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_RESOURCE_CREATE_3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_SUBMIT_3D;
use std::time::Duration;
use std::time::Instant;

#[test]
fn deferred_scanout_moves_readback_off_the_flush_path() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();

    let resp = submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    // Flush responded OK without any backend readback.
    assert!(backend.lock().unwrap().scanout_reads.is_empty());

    // The drain pass of the arming exit skips (fresh guard)...
    dev.gpu.service_deferred_3d_scanout();
    assert!(backend.lock().unwrap().scanout_reads.is_empty());
    // ...and the next drain pass services it.
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);

    let stats = dev.stats();
    assert_eq!(stats.deferred_scanout_flushes, 1);
    assert_eq!(stats.deferred_scanout_serviced, 1);
    assert_eq!(stats.scanout_readbacks, 1);
    assert_eq!(stats.scanout_readback_throttled, 0);
}

#[test]
fn deferred_scanout_coalesces_flushes_into_one_readback() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);

    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().scanout_reads.len(), 1);

    let stats = dev.stats();
    assert_eq!(stats.deferred_scanout_flushes, 3);
    assert_eq!(stats.deferred_scanout_serviced, 1);
}

#[test]
fn iosurface_scanout_blits_on_every_service_while_readback_stays_paced() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    dev.gpu.set_3d_scanout_iosurface(true, false);

    // First frame: blit + readback (readback always due first time).
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    {
        let inner = backend.lock().unwrap();
        assert_eq!(inner.scanout_blits, vec![(31, 1280, 800)]);
        assert_eq!(inner.scanout_reads.len(), 1);
    }

    // With pacing far out, later frames still blit (fresh display) but
    // the CPU readback is withheld.
    dev.gpu
        .set_3d_scanout_readback_interval(Duration::from_secs(60));
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    let inner = backend.lock().unwrap();
    assert_eq!(inner.scanout_blits.len(), 3);
    assert_eq!(inner.scanout_reads.len(), 2);
    drop(inner);
    assert_eq!(dev.stats().scanout_blits, 3);
}

#[test]
fn iosurface_scanout_blits_on_sync_flush_too() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    dev.gpu.set_3d_scanout_deferred(false);
    dev.gpu.set_3d_scanout_iosurface(true, false);

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    let inner = backend.lock().unwrap();
    assert_eq!(inner.scanout_blits.len(), 1);
    assert_eq!(inner.scanout_reads.len(), 1);
}

#[test]
fn deferred_scanout_holds_pending_when_pacing_not_due_instead_of_dropping() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();

    // First flush services immediately (no prior readback).
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().scanout_reads.len(), 1);

    // With a long pacing interval, the next flush stays pending —
    // not dropped, and not counted as throttled.
    dev.gpu
        .set_3d_scanout_readback_interval(Duration::from_secs(60));
    // Re-arm pacing state: interval setter clears last-readback, so
    // perform one readback to start the pacing window.
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().scanout_reads.len(), 2);

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().scanout_reads.len(), 2);
    assert_eq!(dev.stats().scanout_readback_throttled, 0);

    // Dropping the pacing interval lets the held frame service.
    dev.gpu.set_3d_scanout_readback_interval(Duration::ZERO);
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().scanout_reads.len(), 3);
    assert_eq!(dev.stats().deferred_scanout_serviced, 3);
}

#[test]
fn two_d_only_rejects_three_d_and_reports_zero_capsets() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    // virtio_gpu_config: num_scanouts @8 (always 1), num_capsets @12.
    assert_eq!(
        pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 8, 4, &mut mem),
        1
    );
    assert_eq!(
        pci_read(&mut dev, PCI_DEVICE_CFG_OFFSET + 12, 4, &mut mem),
        0
    );
    let mut info = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
    info.extend_from_slice(&0u32.to_le_bytes());
    info.extend_from_slice(&0u32.to_le_bytes());
    let resp = submit_control(&mut dev, &mut mem, &info, 24);
    assert_eq!(
        read_le_u32(&resp, 0),
        Some(virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );
}

#[test]
fn ctx_lifecycle_and_unknown_ctx_errors() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let resp = submit_control(&mut dev, &mut mem, &ctx_create_req(7, 4, b"ctx"), 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(dev.stats().three_d.ctx_active, 1);
    assert_eq!(backend.lock().unwrap().created[0], (7, 4, b"ctx".to_vec()));

    let resp = submit_control(&mut dev, &mut mem, &submit_3d_req(9, &[]), 24);
    assert_eq!(
        read_le_u32(&resp, 0),
        Some(virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_INVALID_PARAMETER)
    );

    let resp = submit_control(
        &mut dev,
        &mut mem,
        &ctrl_req_ctx(VIRTIO_GPU_CMD_CTX_DESTROY, 7),
        24,
    );
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(dev.stats().three_d.ctx_active, 0);
    assert_eq!(backend.lock().unwrap().destroyed, vec![7]);
}

#[test]
fn submit_3d_gathers_split_readable_descriptors() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
    let mut prefix = ctrl_req_ctx(VIRTIO_GPU_CMD_SUBMIT_3D, 1);
    prefix.extend_from_slice(&6u32.to_le_bytes());
    prefix.extend_from_slice(&0u32.to_le_bytes());
    let suffix = [1u8, 2, 3, 4, 5, 6];
    let (resp, used_idx) =
        submit_control_readable_descs(&mut dev, &mut mem, &[&prefix, &suffix], 24);
    assert_eq!(used_idx, 2);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(backend.lock().unwrap().submits, vec![(1, suffix.to_vec())]);
}

#[test]
fn fence_defers_used_ring_until_mock_signals_with_ring_idx() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
    let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 42);
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    let (_resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);
    assert_eq!(used_idx, 1);
    assert_eq!(dev.stats().three_d.fences_pending, 1);
    let pending_capacity = dev.gpu.pending_fenced.capacity();
    let pending_ptr = dev.gpu.pending_fenced.as_ptr();
    let parked_desc_capacity = dev.gpu.pending_fenced[0].descs.capacity();
    let parked_response_capacity = dev.gpu.pending_fenced[0].response.capacity();
    assert!(pending_capacity >= 1);
    assert!(parked_desc_capacity >= 2);
    assert!(parked_response_capacity >= 24);
    assert_eq!(
        backend.lock().unwrap().fences,
        vec![CompletedFence {
            ctx_id: 1,
            ring_idx: 3,
            fence_id: 42
        }]
    );

    backend.lock().unwrap().completed.push(CompletedFence {
        ctx_id: 1,
        ring_idx: 2,
        fence_id: 42,
    });
    dev.drain_completed_fences(&mut mem);
    assert_eq!(dev.stats().three_d.fences_pending, 1);
    assert_eq!(dev.gpu.pending_fenced.capacity(), pending_capacity);
    assert_eq!(dev.gpu.pending_fenced.as_ptr(), pending_ptr);
    let completed_capacity = dev.gpu.completed_fences_scratch.capacity();
    let completed_ptr = dev.gpu.completed_fences_scratch.as_ptr();
    assert!(completed_capacity >= 1);

    backend.lock().unwrap().completed.push(CompletedFence {
        ctx_id: 1,
        ring_idx: 3,
        fence_id: 42,
    });
    dev.drain_completed_fences(&mut mem);
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    assert_eq!(dev.gpu.pending_fenced.capacity(), pending_capacity);
    assert_eq!(dev.gpu.pending_fenced.as_ptr(), pending_ptr);
    assert_eq!(
        dev.gpu.completed_fences_scratch.capacity(),
        completed_capacity
    );
    assert_eq!(dev.gpu.completed_fences_scratch.as_ptr(), completed_ptr);
    assert!(dev.gpu.descriptor_scratch.capacity() >= parked_desc_capacity);
    assert!(dev.gpu.response_scratch.capacity() >= parked_response_capacity);
    assert!(dev.gpu.response_scratch.is_empty());
    assert_eq!(
        u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
        2
    );
}

#[test]
fn completed_fence_buffers_pool_reuses_multiple_parked_responses() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x40000);
    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);

    let mut req1 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 42);
    req1.extend_from_slice(&0u32.to_le_bytes());
    req1.extend_from_slice(&0u32.to_le_bytes());
    let mut req2 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 43);
    req2.extend_from_slice(&0u32.to_le_bytes());
    req2.extend_from_slice(&0u32.to_le_bytes());

    let (_resp, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&req1],
        24,
        0x4000_1000,
        0x4000_4000,
        0x4000_9000,
    );
    assert_eq!(used_idx, 1);
    let (_resp, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&req2],
        24,
        0x4000_1400,
        0x4000_6000,
        0x4000_a000,
    );
    assert_eq!(used_idx, 1);
    assert_eq!(dev.stats().three_d.fences_pending, 2);

    let parked_desc_ptrs = [
        dev.gpu.pending_fenced[0].descs.as_ptr(),
        dev.gpu.pending_fenced[1].descs.as_ptr(),
    ];
    let parked_response_ptrs = [
        dev.gpu.pending_fenced[0].response.as_ptr(),
        dev.gpu.pending_fenced[1].response.as_ptr(),
    ];

    backend.lock().unwrap().completed.extend([
        CompletedFence {
            ctx_id: 1,
            ring_idx: 3,
            fence_id: 42,
        },
        CompletedFence {
            ctx_id: 1,
            ring_idx: 3,
            fence_id: 43,
        },
    ]);
    dev.drain_completed_fences(&mut mem);
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    assert_eq!(dev.gpu.parked_descriptor_scratch.len(), 1);
    assert_eq!(dev.gpu.parked_response_scratch.len(), 1);

    let mut req3 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 44);
    req3.extend_from_slice(&0u32.to_le_bytes());
    req3.extend_from_slice(&0u32.to_le_bytes());
    let mut req4 = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 45);
    req4.extend_from_slice(&0u32.to_le_bytes());
    req4.extend_from_slice(&0u32.to_le_bytes());

    let (_resp, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&req3],
        24,
        0x4000_1800,
        0x4000_8000,
        0x4000_b000,
    );
    assert_eq!(used_idx, 3);
    let (_resp, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&req4],
        24,
        0x4000_1c00,
        0x4000_c000,
        0x4000_d000,
    );
    assert_eq!(used_idx, 3);
    assert_eq!(dev.stats().three_d.fences_pending, 2);

    let reused_desc_ptrs = [
        dev.gpu.pending_fenced[0].descs.as_ptr(),
        dev.gpu.pending_fenced[1].descs.as_ptr(),
    ];
    let reused_response_ptrs = [
        dev.gpu.pending_fenced[0].response.as_ptr(),
        dev.gpu.pending_fenced[1].response.as_ptr(),
    ];
    for ptr in parked_desc_ptrs {
        assert!(reused_desc_ptrs.contains(&ptr));
    }
    for ptr in parked_response_ptrs {
        assert!(reused_response_ptrs.contains(&ptr));
    }
    assert!(dev.gpu.parked_descriptor_scratch.is_empty());
    assert!(dev.gpu.parked_response_scratch.is_empty());
}

#[test]
fn rejected_fence_completes_immediately_without_pending_response() {
    let (mut dev, backend) = dev_with_mock();
    backend.lock().unwrap().reject_fence_ring = Some(3);
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
    let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 3, 43);
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());

    let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);

    assert_eq!(used_idx, 2);
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(
        backend.lock().unwrap().fences,
        vec![CompletedFence {
            ctx_id: 1,
            ring_idx: 3,
            fence_id: 43,
        }]
    );
}

#[test]
fn reset_drops_parked_fences_without_stale_used_write() {
    let (mut dev, _backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);
    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"ctx"), 24);
    let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 1, 0, 9);
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());
    let (_resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);
    assert_eq!(used_idx, 1);
    assert_eq!(dev.stats().three_d.fences_pending, 1);
    pci_write(&mut dev, COMMON_DEVICE_STATUS, 1, 0, &mut mem);
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    assert_eq!(
        u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
        1
    );
}

#[test]
fn controlq_drains_when_driver_never_writes_queue_size_with_3d_backend() {
    // Reproduces the EDK2 VirtioGpuDxe boot hang: firmware programs the rings
    // and enables the control queue but never writes COMMON_QUEUE_SIZE, so the
    // device's stored size stays at its reset value of 0 even though it reports
    // QUEUE_MAX on read. The control queue must still drain at the advertised
    // maximum; otherwise GET_DISPLAY_INFO never completes and the guest hangs.
    let (mut dev, _backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);

    let desc = 0x4000_1000;
    let avail = 0x4000_2000;
    let used = 0x4000_3000;
    let req = 0x4000_4000;
    let resp = 0x4000_5000;

    // Enable the queue the way firmware does: rings + enable, no size write.
    pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, 0, &mut mem);
    pci_write(&mut dev, COMMON_QUEUE_DESC, 8, desc, &mut mem);
    pci_write(&mut dev, COMMON_QUEUE_DRIVER, 8, avail, &mut mem);
    pci_write(&mut dev, COMMON_QUEUE_DEVICE, 8, used, &mut mem);
    pci_write(&mut dev, COMMON_QUEUE_ENABLE, 2, 1, &mut mem);

    // The device advertises the max size but has recorded nothing internally.
    assert_eq!(
        pci_read(&mut dev, COMMON_QUEUE_SIZE, 2, &mut mem),
        u64::from(QUEUE_MAX)
    );
    assert_eq!(dev.stats().queues[0].size, 0);

    // GET_DISPLAY_INFO: readable request desc chained to a writable response.
    let request = ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO);
    let display_info_len = 24 + 16 * 24;
    mem.write(req, &request);
    write_desc(&mut mem, desc, 0, req, request.len() as u32, DESC_F_NEXT, 1);
    write_desc(&mut mem, desc, 1, resp, display_info_len, DESC_F_WRITE, 0);
    mem.write(avail + 2, &1u16.to_le_bytes());
    mem.write(avail + 4, &0u16.to_le_bytes());
    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);

    // Used ring advanced, response written, and the used-buffer interrupt set.
    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        1
    );
    let response = mem.read(resp, 24);
    assert_eq!(
        read_le_u32(&response, 0),
        Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO)
    );
    assert!(dev.interrupt_line_level());

    // A second bring-up command on the same queue also completes.
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    create.extend_from_slice(&1u32.to_le_bytes());
    create.extend_from_slice(&FORMAT_B8G8R8X8_UNORM.to_le_bytes());
    create.extend_from_slice(&64u32.to_le_bytes());
    create.extend_from_slice(&64u32.to_le_bytes());
    mem.write(req, &create);
    write_desc(&mut mem, desc, 2, req, create.len() as u32, DESC_F_NEXT, 3);
    write_desc(&mut mem, desc, 3, resp, 24, DESC_F_WRITE, 0);
    mem.write(avail + 2, &2u16.to_le_bytes());
    mem.write(avail + 4 + 2, &2u16.to_le_bytes());
    pci_write(&mut dev, PCI_NOTIFY_CFG_OFFSET, 4, 0, &mut mem);

    assert_eq!(
        u16::from_le_bytes(mem.read(used + 2, 2).try_into().unwrap()),
        2
    );
    let response = mem.read(resp, 24);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(dev.stats().resources, 1);
}

#[test]
fn fenced_2d_bringup_command_completes_immediately_with_3d_backend() {
    // Firmware sets VIRTIO_GPU_FLAG_FENCE on its 2D bring-up commands. With the
    // 3D backend attached those must still complete on the used ring in the
    // same notify (they are synchronous), rather than being parked behind a
    // backend fence that no context would retire.
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let req = ctrl_req_fenced(VIRTIO_GPU_CMD_GET_DISPLAY_INFO, 0, 0, 7);
    let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24 + 16 * 24);
    assert_eq!(used_idx, 1);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO));
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    // A 2D command must not have been handed to the backend as a fence.
    assert!(backend.lock().unwrap().fences.is_empty());
}

#[test]
fn fenced_resource_create_3d_completes_without_context_zero_fence() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0, 0, 8);
    for field in [41u32, 2, 1, 0x402, 640, 480, 1, 1, 0, 0, 0, 0] {
        req.extend_from_slice(&field.to_le_bytes());
    }

    let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);

    assert_eq!(used_idx, 1);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    assert!(backend.lock().unwrap().fences.is_empty());
}

#[test]
fn fenced_pre_context_local_copy_completes_without_renderer_fence() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x30000);

    for resource_id in [51u32, 52] {
        let mut create = ctrl_req_ctx(VIRTIO_GPU_CMD_RESOURCE_CREATE_3D, 0);
        for field in [
            resource_id,
            2,
            FORMAT_B8G8R8A8_UNORM,
            0x40080,
            2,
            2,
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

        let backing_addr = 0x4002_0000 + u64::from(resource_id - 51) * 0x100;
        let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
        attach.extend_from_slice(&resource_id.to_le_bytes());
        attach.extend_from_slice(&1u32.to_le_bytes());
        attach.extend_from_slice(&backing_addr.to_le_bytes());
        attach.extend_from_slice(&16u32.to_le_bytes());
        attach.extend_from_slice(&0u32.to_le_bytes());
        assert_eq!(
            read_le_u32(&submit_control(&mut dev, &mut mem, &attach, 24), 0),
            Some(VIRTIO_GPU_RESP_OK_NODATA)
        );
    }
    let src_pixels = [1u8, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
    mem.write(0x4002_0100, &src_pixels);

    let mut command = Vec::new();
    for dword in [17u32 | (13 << 16), 51, 0, 0, 0, 0, 52, 0, 0, 0, 0, 2, 2, 1] {
        command.extend_from_slice(&dword.to_le_bytes());
    }
    let mut submit = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 4, 0, 91);
    submit.extend_from_slice(&(command.len() as u32).to_le_bytes());
    submit.extend_from_slice(&0u32.to_le_bytes());
    submit.extend_from_slice(&command);

    let (response, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&submit], 24);
    assert_eq!(used_idx, 5);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(mem.read(0x4002_0000, 16), src_pixels);
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    let backend = backend.lock().unwrap();
    assert!(backend.fences.is_empty());
    assert!(backend.submits.is_empty());
}

#[test]
fn host_vblank_pacing_parks_empty_context_zero_submits_and_retires_one_per_interval() {
    let (mut dev, backend) = dev_with_mock();
    let interval = Duration::from_millis(8);
    dev.set_vblank_interval(interval);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let request = submit_3d_req(0, &[]);

    let (_, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&request],
        24,
        0x4000_1000,
        0x4000_4000,
        0x4000_9000,
    );
    assert_eq!(used_idx, 0);
    let (_, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&request],
        24,
        0x4000_1400,
        0x4000_6000,
        0x4000_a000,
    );
    assert_eq!(used_idx, 0);
    let (_, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&request],
        24,
        0x4000_1800,
        0x4000_8000,
        0x4000_b000,
    );
    assert_eq!(used_idx, 0);
    assert_eq!(dev.gpu.pending_vblank.len(), 3);
    assert!(backend.lock().unwrap().submits.is_empty());

    let base = Instant::now();
    dev.gpu.drain_host_vblank_at(&mut mem, base);
    assert_eq!(
        u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
        1
    );
    assert_eq!(dev.stats().vblank_paced_count, 1);
    assert_eq!(
        read_le_u32(&mem.read(0x4000_9000, 24), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );

    // A late poll may retire one missed interval, but never catch up in a
    // burst. A second poll at the same host time remains held off.
    let late = base + interval * 10;
    dev.gpu.drain_host_vblank_at(&mut mem, late);
    dev.gpu.drain_host_vblank_at(&mut mem, late);
    assert_eq!(
        u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
        2
    );
    assert_eq!(dev.stats().vblank_paced_count, 2);
    assert_eq!(dev.gpu.pending_vblank.len(), 1);

    dev.gpu.drain_host_vblank_at(&mut mem, late + interval);
    assert_eq!(
        u16::from_le_bytes(mem.read(0x4000_3000 + 2, 2).try_into().unwrap()),
        3
    );
    assert_eq!(dev.stats().vblank_paced_count, 3);
    assert!(dev.gpu.pending_vblank.is_empty());
}

#[test]
fn host_vblank_wake_state_tracks_parking_and_the_absolute_schedule() {
    let (mut dev, _backend) = dev_with_mock();
    let interval = Duration::from_millis(8);
    dev.set_vblank_interval(interval);
    let wake = std::sync::Arc::new(VblankWakeState::new());
    dev.set_vblank_wake(std::sync::Arc::clone(&wake));
    assert!(!wake.parked());
    assert_eq!(wake.time_to_deadline(Instant::now()), None);

    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let request = submit_3d_req(0, &[]);
    let (_, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&request],
        24,
        0x4000_1000,
        0x4000_4000,
        0x4000_9000,
    );
    assert_eq!(used_idx, 0);
    let (_, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&request],
        24,
        0x4000_1400,
        0x4000_6000,
        0x4000_a000,
    );
    assert_eq!(used_idx, 0);

    // Parked with no schedule anchor yet: due immediately so the waker
    // fires and the first retire establishes the anchor.
    assert!(wake.parked());
    assert_eq!(wake.time_to_deadline(Instant::now()), Some(Duration::ZERO));

    let base = Instant::now();
    dev.gpu.drain_host_vblank_at(&mut mem, base);
    assert!(wake.parked());
    assert_eq!(wake.time_to_deadline(base), Some(interval));

    // Retire the second NOP half an interval LATE: the next deadline must
    // come from the absolute schedule (base + 2*interval), not from the
    // late retire time, so wake/drain latency cannot lower the long-run
    // pacing rate.
    let late = base + interval + interval / 2;
    dev.gpu.drain_host_vblank_at(&mut mem, late);
    assert!(!wake.parked());
    assert_eq!(wake.time_to_deadline(late), None);

    // The two earlier retires advanced the used index to 2; the third NOP
    // parks again without adding a used entry.
    let (_, used_idx) = submit_control_readable_descs_at(
        &mut dev,
        &mut mem,
        &[&request],
        24,
        0x4000_1800,
        0x4000_8000,
        0x4000_b000,
    );
    assert_eq!(used_idx, 2);
    assert_eq!(dev.gpu.pending_vblank.len(), 1);
    assert!(wake.parked());
    assert_eq!(
        wake.time_to_deadline(base + interval * 2),
        Some(Duration::ZERO)
    );
    assert_eq!(
        wake.time_to_deadline(base + interval + interval * 3 / 4),
        Some(interval / 4)
    );

    // Device reset drops parked NOPs and must quiesce the waker.
    dev.reset_runtime_state();
    assert!(!wake.parked());
    assert_eq!(wake.time_to_deadline(Instant::now()), None);
}

#[test]
fn fenced_empty_context_zero_submit_completes_without_backend_fence() {
    let (mut dev, backend) = dev_with_mock();
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let mut req = ctrl_req_fenced(VIRTIO_GPU_CMD_SUBMIT_3D, 0, 0, 9);
    req.extend_from_slice(&0u32.to_le_bytes());
    req.extend_from_slice(&0u32.to_le_bytes());

    let (resp, used_idx) = submit_control_readable_descs(&mut dev, &mut mem, &[&req], 24);

    assert_eq!(used_idx, 1);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    assert_eq!(dev.stats().three_d.fences_pending, 0);
    assert!(backend.lock().unwrap().fences.is_empty());
}
