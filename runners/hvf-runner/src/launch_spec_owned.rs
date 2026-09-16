//! Signal-aware owner of the complete typed helper and swtpm lifetime.

use super::{default_firmware_code, default_receipt_path};
use anyhow::{bail, Context, Result};
use bridgevm_hvf_runtime::{
    prepare, run_prepared_vm, start_swtpm_controlled, DeviceSurfaces, GpuSurface, HelperLaunch,
    LaunchManifest, RunTermination, RuntimeControl, ShareSurface, VtpmConfig,
};
use std::io::Read;
use std::path::Path;
#[path = "cancellation_signals.rs"]
mod signals;

pub(super) fn run(manifest: LaunchManifest, args: &crate::Args, helper: &Path) -> Result<()> {
    // No child exists while legacy stdin key material may wait for EOF.
    // Default TERM/INT behavior still interrupts this pre-owner read.
    let state_key = if args.helper_vtpm_state.is_some() && args.helper_vtpm_key_stdin {
        let mut key = Vec::new();
        std::io::stdin()
            .take(33)
            .read_to_end(&mut key)
            .context("read vTPM state key")?;
        if key.len() != 32 {
            bail!("vTPM state key must be exactly 32 bytes");
        }
        Some(key)
    } else {
        None
    };
    let _signals =
        signals::CancellationSignals::install().context("install runtime cancellation")?;
    let on_unconfirmed = |value: bridgevm_hvf_runtime::CleanupUnconfirmed| {
        eprintln!(
            "owned_cleanup=unconfirmed role={:?} pid={} reason={} leases=retained",
            value.role, value.pid, value.reason
        );
    };
    let control = RuntimeControl::new(&signals::requested, &on_unconfirmed);
    let prepared = prepare(manifest, "hvf-runner --launch-spec")
        .map_err(|error| anyhow::anyhow!("launch refused: {error}"))?;
    let mut launch = HelperLaunch {
        helper: helper.to_path_buf(),
        firmware_code: args
            .helper_firmware
            .clone()
            .unwrap_or_else(default_firmware_code),
        watchdog_ms: args.watchdog_ms,
        agent_control: args.helper_agent_control.clone(),
        surfaces: args.helper_evidence_dir.as_ref().map(|dir| DeviceSurfaces {
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
            hda_audio: args.helper_hda,
        }),
        swtpm_sockets: None,
    };
    let mut swtpm = None;
    let result = (|| -> Result<_> {
        if let Some(state_dir) = &args.helper_vtpm_state {
            let process = start_swtpm_controlled(
                &VtpmConfig {
                    state_dir: state_dir.clone(),
                    swtpm_bin: args
                        .helper_swtpm_bin
                        .clone()
                        .unwrap_or_else(|| "/opt/homebrew/bin/swtpm".into()),
                    state_key,
                },
                &control,
            )
            .map_err(|error| anyhow::anyhow!("vTPM start failed: {error}"))?;
            launch.swtpm_sockets = Some((
                process.data_socket().to_path_buf(),
                process.control_socket().to_path_buf(),
            ));
            swtpm = Some(process);
        }
        if control.is_cancelled() {
            return run_prepared_vm(&prepared, &launch, Path::new(""), 0, &control)
                .map_err(|error| anyhow::anyhow!("VM cancelled: {error}"));
        }
        if let Some(surfaces) = &launch.surfaces {
            std::fs::create_dir_all(surfaces.evidence_dir.join("ramfb"))
                .context("create evidence dir")?;
        }
        let receipt = args
            .supervise_receipt
            .clone()
            .unwrap_or_else(default_receipt_path);
        run_prepared_vm(
            &prepared,
            &launch,
            Path::new(&receipt),
            args.supervise_max_cycles,
            &control,
        )
        .map_err(|error| anyhow::anyhow!("vm run failed: {error}"))
    })();
    let cleanup = swtpm
        .as_mut()
        .map(|process| process.shutdown(&control))
        .transpose();
    // Both exact Child handles have now been reaped, on success and failure.
    drop(swtpm);
    drop(prepared);
    let run = match (result, cleanup) {
        (Err(primary), Err(cleanup)) => {
            return Err(primary.context(format!("swtpm cleanup also failed: {cleanup}")))
        }
        (Err(primary), _) => return Err(primary),
        (_, Err(cleanup)) => bail!("swtpm cleanup failed after child reap: {cleanup}"),
        (Ok(value), Ok(_)) => value,
    };
    if run.termination == RunTermination::Cancelled || control.is_cancelled() {
        bail!("runtime cancelled; owned helper/swtpm cleanup observed; guest shutdown unproven");
    }
    for cycle in &run.cycles {
        println!(
            "vm cycle: generation={} helper_pid={}",
            cycle.generation, cycle.pid
        );
    }
    println!(
        "vm run ended: {} generation(s); owned children reaped; guest shutdown unproven",
        run.cycles.len()
    );
    Ok(())
}
