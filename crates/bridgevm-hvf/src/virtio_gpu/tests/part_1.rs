//! Split test module.

use super::super::display::*;
use super::super::*;
use super::helpers::*;
use crate::msix::MsixMessage;
use crate::pcie::VIRTIO_GPU_MSIX_VECTOR_COUNT;
use crate::virtio_gpu_3d::CompletedFence;
use crate::virtio_gpu_3d::VIRTIO_GPU_BLOB_MEM_HOST3D;
use crate::virtio_gpu_3d::VIRTIO_GPU_CMD_GET_CAPSET_INFO;

#[test]
fn edid_preferred_timing_is_120_hz_with_valid_ranges_and_checksum() {
    let edid = build_edid(1280, 800);
    let dtd = &edid[54..72];
    let pixel_clock_10khz = u32::from(u16::from_le_bytes([dtd[0], dtd[1]]));
    let h_active = u32::from(dtd[2]) | (u32::from(dtd[4] >> 4) << 8);
    let h_blank = u32::from(dtd[3]) | (u32::from(dtd[4] & 0x0f) << 8);
    let v_active = u32::from(dtd[5]) | (u32::from(dtd[7] >> 4) << 8);
    let v_blank = u32::from(dtd[6]) | (u32::from(dtd[7] & 0x0f) << 8);
    let refresh_hz = pixel_clock_10khz * 10_000 / ((h_active + h_blank) * (v_active + v_blank));

    assert_eq!((h_active, v_active), (1280, 800));
    assert_eq!(refresh_hz, 119); // Integer 10 kHz clock encoding rounds just below 120 Hz.
    assert_eq!(&edid[75..82], &[0xfd, 0, 48, 144, 30, 160, 15]);
    assert_eq!(
        edid.iter().fold(0u8, |sum, byte| sum.wrapping_add(*byte)),
        0
    );
}

#[test]
fn trace_sampling_keeps_initial_evidence_and_sparse_long_run_checkpoints() {
    assert!(trace_sample(1));
    assert!(trace_sample(64));
    assert!(!trace_sample(65));
    assert!(!trace_sample(1023));
    assert!(trace_sample(1024));
    assert!(!trace_sample(1025));
}

#[test]
fn hex_prefix_formats_bounded_payloads() {
    assert_eq!(hex_prefix(&[], 32), "");
    assert_eq!(hex_prefix(&[0x00, 0x0f, 0xa5], 32), "00 0f a5");
    assert_eq!(hex_prefix(&[0x00, 0x01, 0x02, 0x03], 3), "00 01 02 ...");
    assert_eq!(hex_prefix(&[0x7f], 0), " ...");
}

#[test]
fn modern_driver_common_config_sequence_advertises_and_enables_both_queues() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_GPU_F_EDID)
    );
    pci_write(&mut dev, COMMON_DEVICE_FEATURE_SELECT, 4, 1, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_DEVICE_FEATURE, 4, &mut mem),
        u64::from(VIRTIO_F_VERSION_1)
    );
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 0, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE_SELECT, 4, 1, &mut mem);
    pci_write(&mut dev, COMMON_DRIVER_FEATURE, 4, 0xffff_ffff, &mut mem);
    assert_eq!(
        dev.stats().driver_features,
        u64::from(VIRTIO_GPU_F_EDID) | (u64::from(VIRTIO_F_VERSION_1) << 32)
    );
    setup_queue(
        &mut dev,
        &mut mem,
        0,
        0x4000_1000,
        0x4000_2000,
        0x4000_3000,
        0,
    );
    setup_queue(
        &mut dev,
        &mut mem,
        1,
        0x4000_4000,
        0x4000_5000,
        0x4000_6000,
        1,
    );
    let stats = dev.stats();
    assert_eq!(stats.queues[0].size, 16);
    assert!(stats.queues[0].ready);
    assert_eq!(stats.queues[1].size, 16);
    assert!(stats.queues[1].ready);
}

#[test]
fn viogpu3d_msix_contract_accepts_config_control_and_cursor_vectors() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    pci_write(&mut dev, COMMON_CONFIG_MSIX_VECTOR, 2, 0, &mut mem);
    assert_eq!(
        pci_read(&mut dev, COMMON_CONFIG_MSIX_VECTOR, 2, &mut mem),
        0
    );

    for (queue, vector) in [(0u16, 1u16), (1, 2)] {
        pci_write(&mut dev, COMMON_QUEUE_SELECT, 2, u64::from(queue), &mut mem);
        pci_write(
            &mut dev,
            COMMON_QUEUE_MSIX_VECTOR,
            2,
            u64::from(vector),
            &mut mem,
        );
        assert_eq!(
            pci_read(&mut dev, COMMON_QUEUE_MSIX_VECTOR, 2, &mut mem),
            u64::from(vector)
        );
    }

    pci_write(
        &mut dev,
        COMMON_QUEUE_MSIX_VECTOR,
        2,
        u64::from(VIRTIO_GPU_MSIX_VECTOR_COUNT),
        &mut mem,
    );
    assert_eq!(
        pci_read(&mut dev, COMMON_QUEUE_MSIX_VECTOR, 2, &mut mem),
        u64::from(VIRTIO_MSI_NO_VECTOR)
    );
}

#[test]
fn trace_recorder_writes_command_details_for_p3_gpu_bringup() {
    let path = trace_test_path("p3-command-details");
    let (mut dev, _backend) = dev_with_mock();
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let capset_req = {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req
    };
    let _ = submit_control(&mut dev, &mut mem, &capset_req, 40);
    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"venus"), 24);
    let _ = submit_control(
        &mut dev,
        &mut mem,
        &create_blob_req(7, VIRTIO_GPU_BLOB_MEM_HOST3D, 4096, &[]),
        24,
    );
    let _ = submit_control(
        &mut dev,
        &mut mem,
        &submit_3d_req(1, &[0xaa, 0xbb, 0xcc, 0xdd]),
        24,
    );
    drop(dev);

    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(contents.contains("\"event\":\"queue_notify\""));
    assert!(contents.contains("\"name\":\"GET_CAPSET_INFO\""));
    assert!(contents.contains("\"capset_index\":0"));
    assert!(contents.contains("\"response_name\":\"OK_CAPSET_INFO\""));
    assert!(contents.contains("\"response_capset_id\":4"));
    assert!(contents.contains("\"response_capset_max_size\""));
    assert!(contents.contains("\"name\":\"CTX_CREATE\""));
    assert!(contents.contains("\"context_init\":4"));
    assert!(contents.contains("\"debug_name\":\"venus\""));
    assert!(contents.contains("\"name\":\"RESOURCE_CREATE_BLOB\""));
    assert!(contents.contains("\"resource_id\":7"));
    assert!(contents.contains("\"blob_mem\":2"));
    assert!(contents.contains("\"blob_size\":4096"));
    assert!(contents.contains("\"name\":\"SUBMIT_3D\""));
    assert!(contents.contains("\"submit_prefix_hex\":\"aa bb cc dd\""));
}

#[test]
fn trace_never_samples_away_nonempty_submits() {
    let path = trace_test_path("nonempty-submit-sampling");
    let (mut dev, _backend) = dev_with_mock();
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    // Deep past the always-record window: a boot's 60 Hz vsync no-ops put
    // real application submissions thousands deep into this counter.
    dev.gpu.trace_submit_success_count = 5000;
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(1, 4, b"venus"), 24);
    let _ = submit_control(&mut dev, &mut mem, &submit_3d_req(0, &[]), 24);
    let _ = submit_control(
        &mut dev,
        &mut mem,
        &submit_3d_req(1, &[0x11, 0x22, 0x33, 0x44]),
        24,
    );
    drop(dev);

    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    // The empty synchronization no-op is sampled out at this depth...
    assert!(!contents.contains("\"submit_size\":0"));
    // ...but the nonempty application submission must always be recorded.
    assert!(contents.contains("\"submit_prefix_hex\":\"11 22 33 44\""));
}

#[test]
fn trace_command_reuses_field_scratch_across_records() {
    let path = trace_test_path("command-field-scratch");
    let (mut dev, _backend) = dev_with_mock();
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);
    let req = {
        let mut req = ctrl_req(VIRTIO_GPU_CMD_GET_CAPSET_INFO);
        req.extend_from_slice(&0u32.to_le_bytes());
        req.extend_from_slice(&0u32.to_le_bytes());
        req
    };

    let _ = submit_control(&mut dev, &mut mem, &req, 40);
    let cap = dev.gpu.trace_fields_scratch.capacity();
    let ptr = dev.gpu.trace_fields_scratch.as_ptr();
    assert!(cap > 0);
    assert!(dev.gpu.trace_fields_scratch.is_empty());

    let _ = submit_control(&mut dev, &mut mem, &req, 40);

    assert_eq!(dev.gpu.trace_fields_scratch.capacity(), cap);
    assert_eq!(dev.gpu.trace_fields_scratch.as_ptr(), ptr);
    assert!(dev.gpu.trace_fields_scratch.is_empty());
    drop(dev);
    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(contents.matches("\"event\":\"command\"").count() >= 2);
}

#[test]
fn trace_non_command_events_reuse_field_scratch() {
    let path = trace_test_path("non-command-field-scratch");
    let mut dev = VirtioPciGpu::new(1600, 900);
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::disabled();
    dev.gpu.trace_fields_scratch = String::new();

    dev.gpu.write_status(1);
    dev.gpu.write_driver_features(0xffff);
    dev.gpu.trace_queue_notify(42);
    assert_eq!(dev.gpu.trace_fields_scratch.capacity(), 0);

    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    dev.gpu.write_status(1);
    dev.gpu.write_driver_features(0xffff);
    dev.gpu.trace_common_read(REG_STATUS, 4, 1);
    dev.gpu.trace_queue_notify(42);
    dev.gpu.trace_fence_create(
        CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        },
        true,
        "accepted",
    );
    dev.gpu.trace_fence_complete(CompletedFence {
        ctx_id: 1,
        ring_idx: 2,
        fence_id: 3,
    });
    dev.gpu.trace_fence_delivery(
        CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        },
        24,
    );
    let cap = dev.gpu.trace_fields_scratch.capacity();
    let ptr = dev.gpu.trace_fields_scratch.as_ptr();
    assert!(cap > 0);
    assert!(dev.gpu.trace_fields_scratch.is_empty());

    dev.gpu.write_status(1);
    dev.gpu.write_driver_features(0xffff);
    dev.gpu.trace_common_read(REG_STATUS, 4, 1);
    dev.gpu.trace_queue_notify(42);
    dev.gpu.trace_fence_create(
        CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        },
        true,
        "accepted",
    );
    dev.gpu.trace_fence_complete(CompletedFence {
        ctx_id: 1,
        ring_idx: 2,
        fence_id: 3,
    });
    dev.gpu.trace_fence_delivery(
        CompletedFence {
            ctx_id: 1,
            ring_idx: 2,
            fence_id: 3,
        },
        24,
    );

    assert_eq!(dev.gpu.trace_fields_scratch.capacity(), cap);
    assert_eq!(dev.gpu.trace_fields_scratch.as_ptr(), ptr);
    assert!(dev.gpu.trace_fields_scratch.is_empty());
    drop(dev);
    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(contents.contains("\"event\":\"device_status\""));
    assert!(contents.contains("\"event\":\"driver_features\""));
    assert!(contents.contains("\"event\":\"common_read\""));
    assert!(contents.contains("\"event\":\"queue_notify\""));
    assert!(contents.contains("\"event\":\"fence_create\""));
    assert!(contents.contains("\"event\":\"fence_complete\""));
    assert!(contents.contains("\"event\":\"fence_deliver\""));
}

#[test]
fn get_display_info_reports_configured_scanout() {
    let mut dev = VirtioPciGpu::new(1600, 900);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let resp = submit_control(
        &mut dev,
        &mut mem,
        &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
        408,
    );
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO));
    assert_eq!(read_le_u32(&resp, 24 + 8), Some(1600));
    assert_eq!(read_le_u32(&resp, 24 + 12), Some(900));
    assert_eq!(read_le_u32(&resp, 24 + 16), Some(1));
}

#[test]
fn trace_records_display_edid_and_pre_reset_state() {
    let path = trace_test_path("display-edid-reset-details");
    let mut dev = VirtioPciGpu::new(1600, 900);
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);

    let _ = submit_control(
        &mut dev,
        &mut mem,
        &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
        408,
    );
    let mut edid_request = ctrl_req(VIRTIO_GPU_CMD_GET_EDID);
    edid_request.extend_from_slice(&0u32.to_le_bytes());
    edid_request.extend_from_slice(&0u32.to_le_bytes());
    let _ = submit_control(&mut dev, &mut mem, &edid_request, 1056);
    dev.gpu.write_driver_features(u64::MAX);
    dev.gpu.write_status(0xf);
    dev.gpu.write_status(0);

    drop(dev);
    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(contents.contains("\"response_scanout0_width\":1600"));
    assert!(contents.contains("\"response_scanout0_height\":900"));
    assert!(contents.contains("\"response_scanout0_enabled\":1"));
    assert!(contents.contains("\"response_edid_size\":128"));
    assert!(contents.contains("\"response_edid_checksum_valid\":true"));
    assert!(contents
        .contains("\"readable_descriptor_lengths\":[24],\"writable_descriptor_lengths\":[408]"));
    assert!(contents
        .contains("\"readable_descriptor_lengths\":[32],\"writable_descriptor_lengths\":[1056]"));
    assert!(contents.contains(
        "\"writable_descriptor_bytes\":1056,\"response_planned_write_len\":1056,\"response_truncated\":false"
    ));
    assert!(contents.contains(
        "\"response_header_valid\":true,\"response_flags\":0,\"response_fenced\":false,\"response_fence_id\":0,\"response_ctx_id\":0,\"response_ring_idx\":0"
    ));
    assert!(contents.contains("\"raw\":0,\"raw_hex\":\"0x0\",\"previous\":15"));
    assert!(contents.contains("\"driver_features_word0_hex\":\"0x2\""));
    assert!(contents.contains("\"reset\":true"));
}

#[test]
fn host_resize_reports_new_geometry_and_raises_config_change_interrupt() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    program_config_msix_vector(&mut dev, 0);
    program_msix_vector(&mut dev, 0, 0xfee0_1000, 0x71);

    // Config reads start with no pending display event.
    assert_eq!(dev.display_resolution(), (1280, 800));

    assert!(dev.request_display_resolution(1920, 1080));
    assert_eq!(dev.display_resolution(), (1920, 1080));
    // ISR config-change bit (0x2) is set and events_read advertises DISPLAY.
    assert_eq!(dev.stats().interrupt_status & 0x2, 0x2);

    // The armed config-change interrupt is delivered on the config vector.
    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 0,
            address: 0xfee0_1000,
            data: 0x71,
        }]
    );
    // Delivered once only.
    assert!(dev.drain_pending_msix(true, false).is_empty());

    // GET_DISPLAY_INFO now reports the new geometry.
    let mut mem = TestMem::new(0x4000_0000, 0x40000);
    assert_eq!(
        read_le_u32(
            &submit_control(
                &mut dev,
                &mut mem,
                &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
                408
            ),
            24 + 8
        ),
        Some(1920)
    );

    // A no-op resize to the same geometry does not re-arm.
    assert!(!dev.request_display_resolution(1920, 1080));
    // Out-of-range is rejected.
    assert!(!dev.request_display_resolution(0, 1080));
    assert!(!dev.request_display_resolution(1920, MAX_SCANOUT_DIMENSION + 1));
}

#[test]
fn host_resize_display_event_clears_when_guest_acks() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    assert!(dev.request_display_resolution(1600, 900));

    let mut mem = TestMem::new(0x4000_0000, 0x1000);
    // events_read (config offset 0) advertises the DISPLAY event.
    assert_eq!(
        dev.access(
            PCI_DEVICE_CFG_OFFSET,
            VirtioPciGpuOp::Read { size: 4 },
            &mut mem,
        ),
        VirtioGpuResult::ReadValue(u64::from(VIRTIO_GPU_EVENT_DISPLAY))
    );

    // The driver acks by writing the bit into events_clear (config offset 4).
    assert_eq!(
        dev.access(
            PCI_DEVICE_CFG_OFFSET + 4,
            VirtioPciGpuOp::Write {
                size: 4,
                value: u64::from(VIRTIO_GPU_EVENT_DISPLAY),
            },
            &mut mem,
        ),
        VirtioGpuResult::WriteAck
    );

    // events_read no longer reports the acked event.
    assert_eq!(
        dev.access(
            PCI_DEVICE_CFG_OFFSET,
            VirtioPciGpuOp::Read { size: 4 },
            &mut mem,
        ),
        VirtioGpuResult::ReadValue(0)
    );
}

#[test]
fn control_queue_pending_msix_survives_until_table_entry_is_programmed() {
    let mut dev = VirtioPciGpu::new(1600, 900);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let resp = submit_control(
        &mut dev,
        &mut mem,
        &ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO),
        408,
    );

    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO));
    assert!(dev.stats().queues[0].pending_msix);
    assert_eq!(dev.drain_pending_msix(true, false), Vec::new());
    assert!(dev.stats().queues[0].pending_msix);

    program_msix_vector(&mut dev, 0, 0xfee0_0000, 0x40);

    assert_eq!(
        dev.drain_pending_msix(true, false),
        vec![MsixMessage {
            vector: 0,
            address: 0xfee0_0000,
            data: 0x40,
        }]
    );
    assert!(!dev.stats().queues[0].pending_msix);
}

#[test]
fn get_edid_returns_checksum_valid_base_block() {
    let mut dev = VirtioPciGpu::new(1280, 800);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let resp = submit_control(&mut dev, &mut mem, &ctrl_req(VIRTIO_GPU_CMD_GET_EDID), 1056);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_EDID));
    assert_eq!(read_le_u32(&resp, 24), Some(128));
    let edid = &resp[32..160];
    assert_eq!(&edid[0..8], &[0, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]);
    assert_eq!(
        edid.iter().fold(0u8, |acc, byte| acc.wrapping_add(*byte)),
        0
    );
}

#[test]
fn gather_readable_skips_writable_and_unbacked_descriptors() {
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    mem.write(0x4000_1000, b"head");
    mem.write(0x4000_2000, b"skip");
    mem.write(0x4000_3000, b"tail");

    let mut gathered = Vec::new();
    VirtioGpu::gather_readable_into(
        &mem,
        &[
            Descriptor {
                addr: 0x4000_1000,
                len: 4,
                flags: 0,
                next: 0,
            },
            Descriptor {
                addr: 0x4000_2000,
                len: 4,
                flags: DESC_F_WRITE,
                next: 0,
            },
            Descriptor {
                addr: 0x3fff_ff00,
                len: 4,
                flags: 0,
                next: 0,
            },
            Descriptor {
                addr: 0x4000_3000,
                len: 4,
                flags: 0,
                next: 0,
            },
        ],
        &mut gathered,
    );

    assert_eq!(gathered, b"headtail");
}

#[test]
fn gather_readable_rejects_oversized_guest_length_before_growing_scratch() {
    let mem = TestMem::new(0x4000_0000, 0x1000);
    let mut gathered = Vec::with_capacity(32);
    let capacity = gathered.capacity();

    VirtioGpu::gather_readable_into(
        &mem,
        &[Descriptor {
            addr: 0x4000_0800,
            len: u32::MAX,
            flags: 0,
            next: 0,
        }],
        &mut gathered,
    );

    assert!(gathered.is_empty());
    assert_eq!(gathered.capacity(), capacity);
}

#[test]
fn control_queue_reuses_descriptor_request_and_response_scratch_for_immediate_commands() {
    let mut dev = VirtioPciGpu::new(4, 3);
    let mut mem = TestMem::new(0x4000_0000, 0x20000);
    let request = ctrl_req(VIRTIO_GPU_CMD_GET_DISPLAY_INFO);

    let first = submit_control(&mut dev, &mut mem, &request, 408);
    assert_eq!(
        read_le_u32(&first, 0),
        Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO)
    );
    let first_desc_capacity = dev.gpu.descriptor_scratch.capacity();
    let first_request_capacity = dev.gpu.request_scratch.capacity();
    let first_response_capacity = dev.gpu.response_scratch.capacity();
    let first_response_ptr = dev.gpu.response_scratch.as_ptr();
    assert!(dev.gpu.descriptor_scratch.is_empty());
    assert!(dev.gpu.request_scratch.is_empty());
    assert!(dev.gpu.response_scratch.is_empty());
    assert!(first_desc_capacity >= 2);
    assert!(first_request_capacity >= request.len());
    assert!(first_response_capacity >= first.len());

    let second = submit_control(&mut dev, &mut mem, &request, 408);
    assert_eq!(
        read_le_u32(&second, 0),
        Some(VIRTIO_GPU_RESP_OK_DISPLAY_INFO)
    );
    assert_eq!(dev.gpu.descriptor_scratch.capacity(), first_desc_capacity);
    assert_eq!(dev.gpu.request_scratch.capacity(), first_request_capacity);
    assert_eq!(dev.gpu.response_scratch.capacity(), first_response_capacity);
    assert_eq!(dev.gpu.response_scratch.as_ptr(), first_response_ptr);
    assert!(dev.gpu.descriptor_scratch.is_empty());
    assert!(dev.gpu.request_scratch.is_empty());
    assert!(dev.gpu.response_scratch.is_empty());
}

#[test]
fn attach_backing_reuses_resource_backing_and_preserves_on_malformed_request() {
    let mut gpu = VirtioGpu::new(4, 3);
    let mem = TestMem::new(0x4000_0000, 0x20000);
    let mut response = Vec::new();
    let mut create = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_CREATE_2D);
    create.extend_from_slice(&1u32.to_le_bytes());
    create.extend_from_slice(&FORMAT_B8G8R8A8_UNORM.to_le_bytes());
    create.extend_from_slice(&4u32.to_le_bytes());
    create.extend_from_slice(&3u32.to_le_bytes());
    let hdr = CtrlHdr::parse(&create).unwrap();
    gpu.resource_create_2d_into(&create, Some(hdr), &mut response);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

    let mut attach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    attach.extend_from_slice(&1u32.to_le_bytes());
    attach.extend_from_slice(&2u32.to_le_bytes());
    attach.extend_from_slice(&0x4000_8000u64.to_le_bytes());
    attach.extend_from_slice(&4u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    attach.extend_from_slice(&0x4000_9000u64.to_le_bytes());
    attach.extend_from_slice(&8u32.to_le_bytes());
    attach.extend_from_slice(&0u32.to_le_bytes());
    let hdr = CtrlHdr::parse(&attach).unwrap();
    gpu.attach_backing_into(&mem, &attach, Some(hdr), &mut response);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    let resource = gpu.resources.get(&1).unwrap();
    assert_eq!(
        resource.backing,
        vec![
            BackingEntry {
                addr: 0x4000_8000,
                len: 4
            },
            BackingEntry {
                addr: 0x4000_9000,
                len: 8
            },
        ]
    );
    let backing_ptr = resource.backing.as_ptr();
    let backing_capacity = resource.backing.capacity();

    let mut malformed = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    malformed.extend_from_slice(&1u32.to_le_bytes());
    malformed.extend_from_slice(&2u32.to_le_bytes());
    malformed.extend_from_slice(&0x4000_a000u64.to_le_bytes());
    malformed.extend_from_slice(&4u32.to_le_bytes());
    malformed.extend_from_slice(&0u32.to_le_bytes());
    let hdr = CtrlHdr::parse(&malformed).unwrap();
    gpu.attach_backing_into(&mem, &malformed, Some(hdr), &mut response);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_ERR_UNSPEC));
    let resource = gpu.resources.get(&1).unwrap();
    assert_eq!(resource.backing.as_ptr(), backing_ptr);
    assert_eq!(resource.backing.capacity(), backing_capacity);
    assert_eq!(
        resource.backing,
        vec![
            BackingEntry {
                addr: 0x4000_8000,
                len: 4
            },
            BackingEntry {
                addr: 0x4000_9000,
                len: 8
            },
        ]
    );

    let mut reattach = ctrl_req(VIRTIO_GPU_CMD_RESOURCE_ATTACH_BACKING);
    reattach.extend_from_slice(&1u32.to_le_bytes());
    reattach.extend_from_slice(&2u32.to_le_bytes());
    reattach.extend_from_slice(&0x4000_b000u64.to_le_bytes());
    reattach.extend_from_slice(&16u32.to_le_bytes());
    reattach.extend_from_slice(&0u32.to_le_bytes());
    reattach.extend_from_slice(&0x4000_c000u64.to_le_bytes());
    reattach.extend_from_slice(&32u32.to_le_bytes());
    reattach.extend_from_slice(&0u32.to_le_bytes());
    let hdr = CtrlHdr::parse(&reattach).unwrap();
    gpu.attach_backing_into(&mem, &reattach, Some(hdr), &mut response);
    assert_eq!(read_le_u32(&response, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));
    let resource = gpu.resources.get(&1).unwrap();
    assert_eq!(resource.backing.as_ptr(), backing_ptr);
    assert_eq!(resource.backing.capacity(), backing_capacity);
    assert_eq!(
        resource.backing,
        vec![
            BackingEntry {
                addr: 0x4000_b000,
                len: 16
            },
            BackingEntry {
                addr: 0x4000_c000,
                len: 32
            },
        ]
    );
}
