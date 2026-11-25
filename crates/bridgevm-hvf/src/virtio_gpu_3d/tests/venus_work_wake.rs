use super::*;
use std::time::Instant;

#[test]
fn renderer_accepted_venus_payload_arms_the_wake_until_reset() {
    let backend = Arc::new(Mutex::new(MockBackend::new_venus()));
    let mut gpu = VirtioGpu3d::with_backend(Box::new(backend.clone()));
    let wake = Arc::new(VenusWorkWake::default());
    gpu.venus_work_wake = Some(Arc::clone(&wake));
    // Inspect the deadline from the same point even if the runner is descheduled.
    let start = Instant::now();

    let mut context = ctrl_req(VIRTIO_GPU_CMD_CTX_CREATE, 1);
    context.resize(CTX_CREATE_LEN, 0);
    context[28..32].copy_from_slice(&VIRTIO_GPU_CAPSET_VENUS.to_le_bytes());
    let hdr = CtrlHdr3d::parse(&context).unwrap();
    assert_eq!(
        read_le_u32(&gpu.handle(&context, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert!(wake.remaining(start).is_none());

    let mut submit = ctrl_req(VIRTIO_GPU_CMD_SUBMIT_3D, 1);
    submit.extend_from_slice(&4u32.to_le_bytes());
    submit.extend_from_slice(&0u32.to_le_bytes());
    submit.extend_from_slice(&[0x13, 0, 0, 0]);
    let hdr = CtrlHdr3d::parse(&submit).unwrap();
    backend.lock().unwrap().submit_result = Some(Submit3dResult::rejected(-1));
    assert_eq!(
        read_le_u32(&gpu.handle(&submit, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_ERR_UNSPEC)
    );
    assert!(wake.remaining(start).is_none());

    backend.lock().unwrap().submit_result = None;
    assert_eq!(
        read_le_u32(&gpu.handle(&submit, hdr).unwrap(), 0),
        Some(VIRTIO_GPU_RESP_OK_NODATA)
    );
    assert!(wake.remaining(start).is_some());

    gpu.reset();
    assert!(wake.remaining(start).is_none());
}
