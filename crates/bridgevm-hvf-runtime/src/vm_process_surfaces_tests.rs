use super::*;

fn surfaces() -> DeviceSurfaces {
    DeviceSurfaces {
        evidence_dir: "/ev".into(),
        display_export_ms: 100,
        input_control: None,
        virtio_gpu_3d: None,
        aggressive_performance: false,
        nvme_buffered_io: false,
        clipboard_sync: false,
        share: None,
        virtio_net: false,
        hda_audio: false,
        host_diagnostic_stop: false,
    }
}

/// Every value `key` takes in the helper environment, in order.
fn values(surfaces: &DeviceSurfaces, key: &str) -> Vec<String> {
    env(surfaces)
        .into_iter()
        .filter(|(name, _)| *name == key)
        .map(|(_, value)| value)
        .collect()
}

#[test]
fn host_stop_request_is_fixed_and_opt_in() {
    let mut surfaces = surfaces();
    let stop =
        |surfaces: &DeviceSurfaces| values(surfaces, "BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST");
    assert!(stop(&surfaces).is_empty());
    surfaces.host_diagnostic_stop = true;
    assert_eq!(stop(&surfaces), ["/ev/diagnostic-stop.request"]);
}

#[test]
fn the_ramfb_thread_has_its_own_period_and_the_shared_cadence_is_unchanged() {
    let surfaces = surfaces();
    assert_eq!(
        values(&surfaces, "BRIDGEVM_RAMFB_DISPLAY_EXPORT_MS"),
        ["33"]
    );
    assert_eq!(values(&surfaces, "BRIDGEVM_DISPLAY_EXPORT_MS"), ["100"]);
    let readback = values(&surfaces, "BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS");
    assert_eq!(
        readback,
        ["100"],
        "virtio-gpu readback keeps display_export_ms"
    );
}
