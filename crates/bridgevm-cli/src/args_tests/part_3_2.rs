// Additional virtio-gpu trace report tests.

#[test]
fn virtio_gpu_trace_report_groups_bounded_submit_failures_and_accepts_old_jsonl() {
    let path = unique_trace_path("bridgevm-cli-virtio-gpu-submit-failures");
    fs::write(
        &path,
        r#"{"seq":1,"event":"command","name":"SUBMIT_3D","response_name":"ERR_UNSPEC","duration_ns":3000,"ctx_id":7,"submit_size":64,"submit_first_command_id":43,"renderer_status":22,"renderer_command_id":43,"renderer_resource_id":14,"renderer_resource_found":true,"renderer_resource_backed":false}
{"seq":2,"event":"command","name":"SUBMIT_3D","response_name":"ERR_UNSPEC","duration_ns":1000,"ctx_id":7,"submit_size":64,"submit_first_command_id":43,"renderer_status":22,"renderer_command_id":43,"renderer_resource_id":14,"renderer_resource_found":true,"renderer_resource_backed":false}
{"seq":3,"event":"command","name":"SUBMIT_3D","response_name":"ERR_UNSPEC","submit_size":16,"submit_first_command_id":8}
"#,
    )
    .unwrap();

    let report = analyze_virtio_gpu_trace(&path).unwrap();
    let _ = fs::remove_file(path);

    assert_eq!(report.command_percentile_us(50, 100), 1.0);
    assert_eq!(report.command_percentile_us(95, 100), 3.0);
    assert_eq!(report.submit_3d_failures.len(), 2);
    let transfer = report
        .submit_3d_failures
        .iter()
        .find(|(failure, _)| failure.command_id == Some(43))
        .unwrap();
    assert_eq!(*transfer.1, 2);
    assert_eq!(transfer.0.renderer_status, Some(22));
    assert_eq!(transfer.0.resource_id, Some(14));
    assert_eq!(transfer.0.resource_found, Some(true));
    assert_eq!(transfer.0.resource_backed, Some(false));
    let old = report
        .submit_3d_failures
        .iter()
        .find(|(failure, _)| failure.command_id == Some(8))
        .unwrap();
    assert_eq!(*old.1, 1);
    assert_eq!(old.0.renderer_status, None);
}
