//! Explicit device-surface environment for a typed helper generation.
use super::DeviceSurfaces;

pub(super) fn env(surfaces: &DeviceSurfaces) -> Vec<(&'static str, String)> {
    let mut env = Vec::new();
    let evidence = |name: &str| surfaces.evidence_dir.join(name).display().to_string();
    env.push(("BRIDGEVM_RAMFB", "1".to_string()));
    env.push(("BRIDGEVM_RAMFB_DUMP_DIR", evidence("ramfb")));
    // No PPM feed: the app reads display.fb, and the PPM path costs a full
    // frame checksum and file rewrite per interval for a file nobody opens.
    env.push(("BRIDGEVM_DISPLAY_EXPORT_FB", evidence("display.fb")));
    env.push((
        "BRIDGEVM_DISPLAY_EXPORT_MS",
        surfaces.display_export_ms.to_string(),
    ));
    env.push((
        "BRIDGEVM_VIRTIO_GPU_SCANOUT_READBACK_MS",
        surfaces.display_export_ms.to_string(),
    ));
    match &surfaces.input_control {
        Some(control) => {
            env.push(("BRIDGEVM_INPUT_CONTROL", control.display().to_string()));
        }
        None => {
            env.push(("BRIDGEVM_DISABLE_XHCI", "1".to_string()));
        }
    }
    if surfaces.clipboard_sync {
        env.push(("BRIDGEVM_VIRTIO_CONSOLE_CLIPSYNC", "1".to_string()));
    }
    if let Some(share) = &surfaces.share {
        env.push((
            "BRIDGEVM_VIRTIO_CONSOLE_SHARE",
            format!("{}::{}", share.host_dir.display(), share.guest_dir),
        ));
        env.push((
            "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MS",
            share.interval_ms.to_string(),
        ));
        env.push((
            "BRIDGEVM_VIRTIO_CONSOLE_SHARE_MAX_KB",
            share.max_kb.to_string(),
        ));
    }
    if surfaces.virtio_net {
        env.push(("BRIDGEVM_VIRTIO_NET", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_NET_BACKEND", "nat".to_string()));
    }
    if surfaces.hda_audio {
        env.push(("BRIDGEVM_HDA", "1".to_string()));
        env.push(("BRIDGEVM_HDA_COREAUDIO", "1".to_string()));
    }
    if surfaces.nvme_buffered_io {
        env.push(("BRIDGEVM_NVME_BUFFERED_IO", "1".to_string()));
    }
    if let Some(gpu) = &surfaces.virtio_gpu_3d {
        env.push(("BRIDGEVM_VIRTIO_GPU", "1".to_string()));
        env.push(("BRIDGEVM_VIRTIO_GPU_3D", "1".to_string()));
        env.push((
            "BRIDGEVM_VIRTIO_GPU_3D_PROTOCOL",
            if gpu.virgl { "virgl" } else { "venus" }.to_string(),
        ));
        if surfaces.aggressive_performance {
            env.push(("BRIDGEVM_VIRTIO_GPU_DIRECT_RENDERER", "1".to_string()));
            env.push(("BRIDGEVM_VIRTIO_GPU_ASYNC_SCANOUT", "1".to_string()));
            env.push(("BRIDGEVM_VIRTIO_GPU_IOSURFACE_SCANOUT", "1".to_string()));
        }
        if !gpu.virgl {
            // venus needs MoltenVK in process and a BAR2 EDK2 can assign.
            env.push((
                "BRIDGEVM_VULKAN_LIB",
                "/opt/homebrew/lib/libMoltenVK.dylib".to_string(),
            ));
            env.push(("BRIDGEVM_VIRTIO_GPU_HOSTMEM_MIB", "512".to_string()));
        }
        match &gpu.device_id {
            Some(device_id) => env.push((
                "BRIDGEVM_VIRTIO_GPU_PCI_DEVICE_ID",
                format!("0x{device_id}"),
            )),
            None => env.push(("BRIDGEVM_VIRTIO_GPU_3D_BIND_ID", "1".to_string())),
        }
        env.push((
            "BRIDGEVM_VIRTIO_GPU_TRACE_JSONL",
            evidence("virtio-gpu.jsonl"),
        ));
    }
    if surfaces.host_diagnostic_stop {
        env.push((
            "BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST",
            evidence("diagnostic-stop.request"),
        ));
    }
    env
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_stop_request_is_fixed_and_opt_in() {
        let mut surfaces = DeviceSurfaces {
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
        };
        let stop = |items: &[(&str, String)]| {
            items
                .iter()
                .find(|(key, _)| *key == "BRIDGEVM_HOST_DIAGNOSTIC_STOP_REQUEST")
                .map(|(_, value)| value.clone())
        };
        assert_eq!(stop(&env(&surfaces)), None);
        surfaces.host_diagnostic_stop = true;
        assert_eq!(
            stop(&env(&surfaces)),
            Some("/ev/diagnostic-stop.request".to_string())
        );
    }
}
