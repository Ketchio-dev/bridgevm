//! One launch configuration builder shared by legacy and owned wire modes.
use bridgevm_hvf_runtime::{
    AgentUpdateSurface, DeviceSurfaces, GpuSurface, HelperLaunch, ShareSurface,
};
use std::path::Path;
pub(super) fn build(args: &crate::Args, helper: &Path) -> HelperLaunch {
    let firmware = args
        .typed
        .helper_firmware
        .clone()
        .unwrap_or_else(super::default_firmware_code);
    let evidence = args.typed.helper_evidence_dir.as_ref();
    HelperLaunch {
        helper: helper.to_path_buf(),
        firmware_code: firmware,
        watchdog_ms: args.watchdog_ms,
        agent_control: args.typed.helper_agent_control.clone(),
        surfaces: evidence.map(|dir| DeviceSurfaces {
            evidence_dir: dir.clone(),
            display_export_ms: 100,
            input_control: Some(dir.join("input.ctl")),
            virtio_gpu_3d: args.virtio_gpu_3d.then(|| GpuSurface {
                virgl: args.gpu_trace_protocol.as_deref() == Some("virgl"),
                device_id: args.virtio_gpu_device_id.clone(),
            }),
            virtio_gpu_2d: args.virtio_gpu,
            aggressive_performance: args.virtio_gpu_3d,
            nvme_buffered_io: args.nvme_buffered_io,
            nvme_durable_flush: args.nvme_durable_flush,
            guest_resolution: args.guest_resolution.clone(),
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
            net_forwards: args.net_forwards.clone(),
            hda_audio: args.typed.helper_hda,
            hda_microphone: args.typed.helper_hda_microphone,
            // clap requires the marker and the session with the folder.
            agent_update: match &args.typed {
                crate::launch_spec::LaunchSpecArgs {
                    helper_agent_update: Some(dir),
                    helper_agent_update_marker: Some(marker),
                    helper_agent_update_session: Some(session),
                    ..
                } => Some(AgentUpdateSurface {
                    dir: dir.clone(),
                    marker: marker.clone(),
                    session: session.clone(),
                }),
                _ => None,
            },
            guest_tools: args.typed.helper_guest_tools.clone(),
            host_diagnostic_stop: args.typed.helper_host_diagnostic_stop,
        }),
        swtpm_sockets: None,
    }
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use std::path::Path;

    fn surfaces(gpu_flags: &[&str]) -> bridgevm_hvf_runtime::DeviceSurfaces {
        let mut argv = vec!["hvf-runner", "--launch-spec", "m.json", "--helper", "p"];
        argv.extend(["--helper-evidence-dir", "/ev"]);
        argv.extend(gpu_flags);
        let args = crate::Args::try_parse_from(argv).unwrap();
        super::build(&args, Path::new("p")).surfaces.unwrap()
    }

    #[test]
    fn the_owned_launch_accepts_a_disposable_tools_volume() {
        let args = crate::Args::try_parse_from([
            "hvf-runner", "--launch-spec", "m.json", "--helper", "p",
            "--helper-evidence-dir", "/ev", "--helper-guest-tools", "/tools.raw",
        ]);
        assert!(args.is_ok(), "the typed product path must accept the tools image");
        let launch = super::build(&args.unwrap(), Path::new("p"));
        assert_eq!(launch.surfaces.unwrap().guest_tools.unwrap(), Path::new("/tools.raw"));
    }

    #[test]
    fn the_owned_launch_carries_the_port_forwards() {
        assert!(surfaces(&[]).net_forwards.is_empty());
        let forwards = &[
            "--virtio-net",
            "--net-forward",
            "8080:80",
            "--net-forward",
            "2222:22",
        ];
        assert_eq!(surfaces(forwards).net_forwards, [(8080, 80), (2222, 22)]);
        let mut argv = vec!["hvf-runner", "--launch-spec", "m.json", "--helper", "p"];
        argv.extend(["--net-forward", "80"]);
        assert!(crate::Args::try_parse_from(argv).is_err());
    }

    #[test]
    fn the_owned_launch_carries_the_microphone_setting() {
        assert!(!surfaces(&["--helper-hda"]).hda_microphone);
        let on = surfaces(&["--helper-hda", "--helper-hda-microphone"]);
        assert!(on.hda_audio && on.hda_microphone);
    }

    #[test]
    fn the_owned_launch_carries_the_2d_display_device() {
        assert!(!surfaces(&[]).virtio_gpu_2d);
        let two_d = surfaces(&["--virtio-gpu"]);
        assert!(two_d.virtio_gpu_2d);
        assert!(two_d.virtio_gpu_3d.is_none());
        assert!(
            !two_d.aggressive_performance,
            "the 3D renderer lane stays off"
        );
    }

    #[test]
    fn the_owned_launch_carries_the_guest_tools_update() {
        assert!(surfaces(&[]).agent_update.is_none());
        let update = surfaces(&[
            "--helper-agent-update",
            "/app/scripts/win-assets",
            "--helper-agent-update-marker",
            "/vm/metadata/agent-update.json",
            "--helper-agent-update-session",
            "s-1",
        ])
        .agent_update
        .unwrap();
        assert_eq!(update.dir, Path::new("/app/scripts/win-assets"));
        assert_eq!(update.marker, Path::new("/vm/metadata/agent-update.json"));
        assert_eq!(update.session, "s-1");
        let argv = ["hvf-runner", "--launch-spec", "m.json", "--helper", "p"];
        let partial = argv.iter().chain(&["--helper-agent-update", "/app"]);
        assert!(
            crate::Args::try_parse_from(partial).is_err(),
            "marker and session are required"
        );
    }
}
