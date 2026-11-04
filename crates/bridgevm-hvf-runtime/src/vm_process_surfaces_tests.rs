use super::super::GpuSurface;
use super::*;

fn surfaces() -> DeviceSurfaces {
    DeviceSurfaces {
        evidence_dir: "/ev".into(),
        display_export_ms: 100,
        input_control: None,
        virtio_gpu_3d: None,
        virtio_gpu_2d: false,
        aggressive_performance: false,
        nvme_buffered_io: false,
        nvme_durable_flush: false,
        guest_resolution: None,
        clipboard_sync: false,
        share: None,
        virtio_net: false,
        hda_audio: false,
        hda_microphone: false,
        host_diagnostic_stop: false,
    }
}

#[test]
fn the_microphone_opens_only_with_the_users_setting_on_an_audio_vm() {
    let mut surfaces = surfaces();
    let microphone = |surfaces: &DeviceSurfaces| values(surfaces, "BRIDGEVM_HDA_MICROPHONE");
    surfaces.hda_audio = true;
    assert!(microphone(&surfaces).is_empty(), "off by default");
    surfaces.hda_microphone = true;
    assert_eq!(microphone(&surfaces), ["1"]);
    surfaces.hda_audio = false;
    assert!(
        microphone(&surfaces).is_empty(),
        "no HDA device, no microphone"
    );
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

#[test]
fn only_the_users_durable_disk_choice_makes_every_flush_a_full_sync() {
    let mut surfaces = surfaces();
    assert!(values(&surfaces, "BRIDGEVM_NVME_FLUSH").is_empty());
    surfaces.nvme_durable_flush = true;
    assert_eq!(values(&surfaces, "BRIDGEVM_NVME_FLUSH"), ["durable"]);
}

#[test]
fn the_2d_display_device_adds_exactly_the_device_switch() {
    let mut surfaces = surfaces();
    let without = env(&surfaces);
    surfaces.virtio_gpu_2d = true;
    let with = env(&surfaces);
    let added: Vec<_> = with
        .iter()
        .filter(|entry| !without.contains(entry))
        .collect();
    // No 3D backend, PCI id override, bind alias or trace: the default 1AF4:1050 device.
    assert_eq!(added, [&("BRIDGEVM_VIRTIO_GPU", "1".to_string())]);
    assert_eq!(with.len(), without.len() + 1);
}

#[test]
fn three_d_wins_when_both_gpu_surfaces_are_set() {
    let mut surfaces = surfaces();
    surfaces.virtio_gpu_3d = Some(GpuSurface {
        virgl: true,
        device_id: Some("1050".to_string()),
    });
    let three_d = env(&surfaces);
    surfaces.virtio_gpu_2d = true;
    assert_eq!(env(&surfaces), three_d);
    assert_eq!(values(&surfaces, "BRIDGEVM_VIRTIO_GPU"), ["1"]);
    assert_eq!(values(&surfaces, "BRIDGEVM_VIRTIO_GPU_3D"), ["1"]);
}

#[test]
fn only_the_users_resolution_choice_reaches_the_vmm_verbatim() {
    let mut surfaces = surfaces();
    assert!(values(&surfaces, "BRIDGEVM_GUEST_RESOLUTION").is_empty());
    surfaces.guest_resolution = Some("1920x1080".to_string());
    assert_eq!(
        values(&surfaces, "BRIDGEVM_GUEST_RESOLUTION"),
        ["1920x1080"]
    );
}
