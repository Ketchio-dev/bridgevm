// Additional command-trace tests.

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
fn trace_records_bounded_submit_renderer_diagnostics() {
    let path = trace_test_path("submit-renderer-diagnostic");
    let (mut dev, backend) = dev_with_mock();
    backend.lock().unwrap().submit_result = Some(crate::virtio_gpu_3d::Submit3dResult {
        accepted: false,
        diagnostic: Some(crate::virtio_gpu_3d::Submit3dDiagnostic {
            renderer_status: 22,
            command_offset_dwords: Some(17),
            command_id: Some(43),
            command_header: Some(0x000d_002b),
            resource_id: Some(14),
            resource_found: Some(true),
            resource_backed: Some(false),
        }),
    });
    dev.gpu.trace = crate::virtio_gpu_trace::VirtioGpuTraceRecorder::test_file(&path);
    let mut mem = TestMem::new(0x4000_0000, 0x10000);

    let _ = submit_control(&mut dev, &mut mem, &ctx_create_req(7, 2, b"shadow"), 24);
    let response = submit_control(
        &mut dev,
        &mut mem,
        &submit_3d_req(7, &[0x2b, 0x00, 0x0d, 0x00]),
        24,
    );
    assert_eq!(
        read_le_u32(&response, 0),
        Some(crate::virtio_gpu_3d::VIRTIO_GPU_RESP_ERR_UNSPEC)
    );
    drop(dev);

    let contents = std::fs::read_to_string(&path).unwrap();
    let _ = std::fs::remove_file(path);
    assert!(contents.contains("\"renderer_status\":22"));
    assert!(contents.contains("\"renderer_command_offset_dwords\":17"));
    assert!(contents.contains("\"renderer_command_id\":43"));
    assert!(contents.contains("\"renderer_command_header\":852011"));
    assert!(contents.contains("\"renderer_resource_id\":14"));
    assert!(contents.contains("\"renderer_resource_found\":true"));
    assert!(contents.contains("\"renderer_resource_backed\":false"));
}
