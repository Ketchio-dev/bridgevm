//! One launch configuration builder shared by legacy and owned wire modes.
use bridgevm_hvf_runtime::{DeviceSurfaces, GpuSurface, HelperLaunch, ShareSurface};
use std::path::Path;
pub(super) fn build(args: &crate::Args, helper: &Path) -> HelperLaunch {
    HelperLaunch {
        helper: helper.to_path_buf(),
        firmware_code: args
            .typed
            .helper_firmware
            .clone()
            .unwrap_or_else(super::default_firmware_code),
        watchdog_ms: args.watchdog_ms,
        agent_control: args.typed.helper_agent_control.clone(),
        surfaces: args
            .typed
            .helper_evidence_dir
            .as_ref()
            .map(|dir| DeviceSurfaces {
                evidence_dir: dir.clone(),
                display_export_ms: 100,
                input_control: Some(dir.join("input.ctl")),
                virtio_gpu_3d: args.virtio_gpu_3d.then(|| GpuSurface {
                    virgl: args.gpu_trace_protocol.as_deref() == Some("virgl"),
                    device_id: args.virtio_gpu_device_id.clone(),
                }),
                aggressive_performance: args.virtio_gpu_3d,
                nvme_buffered_io: args.nvme_buffered_io,
                clipboard_sync: args.agent_clipboard_sync,
                share: args
                    .agent_share_host
                    .as_ref()
                    .zip(args.agent_share_guest.as_ref())
                    .map(|(host, guest)| ShareSurface {
                        host_dir: host.clone(),
                        guest_dir: guest.clone(),
                        interval_ms: args.agent_share_ms.unwrap_or(2000),
                        max_kb: args.agent_share_max_kb.unwrap_or(65536),
                    }),
                virtio_net: args.virtio_net,
                hda_audio: args.typed.helper_hda,
            }),
        swtpm_sockets: None,
    }
}
