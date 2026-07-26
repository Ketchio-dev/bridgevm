// Split test module: asynchronous scanout present at device level.

#[test]
fn async_present_returns_flush_without_blocking_on_the_renderer() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    backend.lock().unwrap().async_present = true;
    // Stall the worker so the present cannot possibly have completed.
    backend.lock().unwrap().async_present_stall = true;
    dev.gpu.set_3d_scanout_async_present(true);

    let resp = submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    assert_eq!(read_le_u32(&resp, 0), Some(VIRTIO_GPU_RESP_OK_NODATA));

    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();

    // The present was handed to the worker, and the device thread did not wait
    // for it: no readback has landed even though the frame was dispatched.
    assert_eq!(
        backend.lock().unwrap().async_present_starts,
        vec![(31, 1280, 800)]
    );
    assert_eq!(dev.stats().scanout_readbacks, 0);
}

#[test]
fn async_present_applies_the_readback_once_the_worker_finishes() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    backend.lock().unwrap().async_present = true;
    dev.gpu.set_3d_scanout_async_present(true);

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    // A later drain collects the finished present.
    dev.gpu.service_deferred_3d_scanout();

    assert_eq!(
        backend.lock().unwrap().async_present_starts,
        vec![(31, 1280, 800)]
    );
    assert_eq!(dev.stats().scanout_readbacks, 1);
}

#[test]
fn async_present_never_queues_more_than_one_pending_frame() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    backend.lock().unwrap().async_present = true;
    backend.lock().unwrap().async_present_stall = true;
    dev.gpu.set_3d_scanout_async_present(true);

    // Many flushes while the worker is stalled must not accumulate work.
    for _ in 0..50 {
        submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
        dev.gpu.service_deferred_3d_scanout();
        dev.gpu.service_deferred_3d_scanout();
    }

    // Exactly one present is executing; the rest were superseded, not queued.
    assert_eq!(backend.lock().unwrap().async_present_starts.len(), 1);
}

#[test]
fn disabling_async_present_drains_the_worker_before_switching_back() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    backend.lock().unwrap().async_present = true;
    dev.gpu.set_3d_scanout_async_present(true);

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();

    // Switching modes must not abandon a buffer the worker still owns.
    dev.gpu.set_3d_scanout_async_present(false);

    // The synchronous path is authoritative again.
    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    assert!(!backend.lock().unwrap().scanout_reads.is_empty());
}

#[test]
fn a_backend_without_an_async_path_falls_back_to_the_inline_present() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    // async_present stays false: the mock refuses scanout_present_start.
    dev.gpu.set_3d_scanout_async_present(true);

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();

    // No async start was served, and the synchronous readback still happened,
    // so enabling async on an unsupporting backend cannot lose frames.
    assert!(backend.lock().unwrap().async_present_starts.is_empty());
    assert_eq!(backend.lock().unwrap().scanout_reads, vec![(31, 1280, 800)]);
}

#[test]
fn async_present_survives_a_scanout_change_without_applying_a_stale_frame() {
    let (mut dev, backend, mut mem) = deferred_scanout_dev();
    backend.lock().unwrap().async_present = true;
    backend.lock().unwrap().async_present_stall = true;
    dev.gpu.set_3d_scanout_async_present(true);

    submit_control(&mut dev, &mut mem, &flush_res_31(), 24);
    dev.gpu.service_deferred_3d_scanout();
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(backend.lock().unwrap().async_present_starts.len(), 1);

    // Unbind the scanout while the present is still in flight.
    let mut unbind = ctrl_req(VIRTIO_GPU_CMD_SET_SCANOUT);
    for field in [0u32, 0, 1280, 800, 0, 0] {
        unbind.extend_from_slice(&field.to_le_bytes());
    }
    submit_control(&mut dev, &mut mem, &unbind, 24);

    // Let the stalled present finish and be collected; it must be discarded
    // rather than composited onto the now-different scanout.
    backend.lock().unwrap().async_present_stall = false;
    dev.gpu.service_deferred_3d_scanout();
    assert_eq!(dev.stats().scanout_readbacks, 0);
}
