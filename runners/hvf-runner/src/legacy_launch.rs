//! Debug-only bridge from the old CLI to the source-checkout shell wrapper.
use crate::*;
use anyhow::{bail, Context, Result};
use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

pub(crate) fn launch_installed_windows(args: &Args) -> Result<()> {
    let invocation_dir =
        env::current_dir().context("resolve current directory for hvf-runner --launch")?;
    let repo_root = launch_repo_root(args, &invocation_dir);
    let wrapper = repo_root.join("scripts/run-hvf-windows-installed-boot.sh");
    if !wrapper.is_file() {
        bail!(
            "installed Windows HVF boot wrapper not found at {}; pass --repo-root or run from the repository root",
            wrapper.display()
        );
    }

    let wrapper_args = installed_boot_launch_args(args, &invocation_dir)?;
    let status = Command::new(&wrapper)
        .args(&wrapper_args)
        .current_dir(&repo_root)
        .status()
        .with_context(|| format!("launch installed Windows HVF wrapper {}", wrapper.display()))?;
    if status.success() {
        Ok(())
    } else {
        bail!("installed Windows HVF boot wrapper failed with status {status}")
    }
}

pub(crate) fn launch_repo_root(args: &Args, invocation_dir: &Path) -> PathBuf {
    if let Some(repo_root) = &args.repo_root {
        return resolve_launch_path(repo_root, invocation_dir);
    }
    if let Ok(repo_root) = env::var("BRIDGEVM_REPO_ROOT") {
        if !repo_root.trim().is_empty() {
            return resolve_launch_path(Path::new(&repo_root), invocation_dir);
        }
    }
    invocation_dir.to_path_buf()
}

pub(crate) fn installed_boot_launch_args(
    args: &Args,
    invocation_dir: &Path,
) -> Result<Vec<String>> {
    if args.boot_timer_desktop_agent && args.boot_timer_desktop_checksum64.is_some() {
        bail!("choose exactly one BOOT_TIMER desktop oracle: --boot-timer-desktop-agent or --boot-timer-desktop-checksum64");
    }
    let agent_service = args.agent_service_control.is_some();
    let agent_extras = args.agent_service_command.is_some()
        || args.agent_clipboard_sync
        || args.agent_share_host.is_some()
        || args.agent_share_guest.is_some()
        || args.agent_share_ms.is_some()
        || args.agent_share_max_kb.is_some();
    if !agent_service && agent_extras {
        bail!("agent command, clipboard, and share options require --agent-service-control");
    }
    if agent_service
        && (args.shutdown_after_agent_ready || args.host_pause_resume_proof_ms.is_some())
    {
        bail!("--agent-service-control cannot be combined with one-shot shutdown or host pause/resume proof controls");
    }
    if args.agent_share_host.is_some() != args.agent_share_guest.is_some() {
        bail!("--agent-share-host and --agent-share-guest must be provided together");
    }
    if args.agent_share_ms.is_some() && args.agent_share_host.is_none() {
        bail!("--agent-share-ms requires --agent-share-host and --agent-share-guest");
    }
    if args
        .agent_share_max_kb
        .is_some_and(|max_kb| !(1..=1_048_576).contains(&max_kb))
    {
        bail!("--agent-share-max-kb requires an integer from 1 to 1048576");
    }
    if args.agent_share_max_kb.is_some() && args.agent_share_host.is_none() {
        bail!("--agent-share-max-kb requires --agent-share-host and --agent-share-guest");
    }
    let mut target_candidates = [
        args.target.as_ref(),
        args.disk.as_ref(),
        args.writable_disk.as_ref(),
    ]
    .into_iter()
    .flatten();
    let target = target_candidates
        .next()
        .ok_or_else(|| anyhow::anyhow!("--launch requires --target, --disk, or --writable-disk"))?;
    if target_candidates.next().is_some() {
        bail!("--launch accepts only one of --target, --disk, or --writable-disk");
    }
    let vars = args
        .vars
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--launch requires --vars"))?;
    let evidence_dir = args
        .evidence_dir
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("--launch requires --evidence-dir"))?;

    let mut out = vec![
        "--target".to_string(),
        path_arg(target, invocation_dir),
        "--vars".to_string(),
        path_arg(vars, invocation_dir),
        "--evidence-dir".to_string(),
        path_arg(evidence_dir, invocation_dir),
    ];

    push_path_arg(
        &mut out,
        "--placeholder-nsid1",
        args.placeholder_nsid1.as_ref(),
        invocation_dir,
    );
    push_num_arg(&mut out, "--watchdog-ms", args.watchdog_ms);
    push_flag(&mut out, args.no_watchdog, "--no-watchdog");
    push_num_arg(&mut out, "--max-reboots", args.max_reboots);
    push_num_arg(&mut out, "--ram-mib", args.ram_mib);
    push_num_arg(&mut out, "--smp-cpus", args.smp_cpus);
    push_flag(&mut out, args.boot_timer, "--boot-timer");
    push_num_arg(&mut out, "--boot-timer-ramfb-ms", args.boot_timer_ramfb_ms);
    push_string_arg(
        &mut out,
        "--boot-timer-desktop-checksum64",
        args.boot_timer_desktop_checksum64.as_deref(),
    );
    push_flag(
        &mut out,
        args.boot_timer_desktop_agent,
        "--boot-timer-desktop-agent",
    );
    push_flag(
        &mut out,
        args.shutdown_after_agent_ready,
        "--shutdown-after-agent-ready",
    );
    push_num_arg(
        &mut out,
        "--host-pause-resume-proof-ms",
        args.host_pause_resume_proof_ms,
    );
    push_path_arg(
        &mut out,
        "--agent-service-control",
        args.agent_service_control.as_ref(),
        invocation_dir,
    );
    push_string_arg(
        &mut out,
        "--agent-service-command",
        args.agent_service_command.as_deref(),
    );
    push_flag(
        &mut out,
        args.agent_clipboard_sync,
        "--agent-clipboard-sync",
    );
    push_path_arg(
        &mut out,
        "--agent-share-host",
        args.agent_share_host.as_ref(),
        invocation_dir,
    );
    push_string_arg(
        &mut out,
        "--agent-share-guest",
        args.agent_share_guest.as_deref(),
    );
    push_num_arg(&mut out, "--agent-share-ms", args.agent_share_ms);
    push_num_arg(&mut out, "--agent-share-max-kb", args.agent_share_max_kb);
    push_flag(&mut out, args.enable_xhci, "--enable-xhci");
    push_flag(&mut out, args.virtio_net, "--virtio-net");
    push_flag(&mut out, args.nvme_buffered_io, "--nvme-buffered-io");
    push_flag(&mut out, args.virtio_gpu_3d, "--virtio-gpu-3d");
    push_string_arg(
        &mut out,
        "--virtio-gpu-device-id",
        args.virtio_gpu_device_id.as_deref(),
    );
    push_path_arg(
        &mut out,
        "--gpu-trace",
        args.gpu_trace.as_ref(),
        invocation_dir,
    );
    push_string_arg(
        &mut out,
        "--gpu-trace-protocol",
        args.gpu_trace_protocol.as_deref(),
    );
    push_flag(
        &mut out,
        args.require_gpu_trace_gate,
        "--require-gpu-trace-gate",
    );
    push_path_arg(
        &mut out,
        "--viogpu3d-dir",
        args.viogpu3d_dir.as_ref(),
        invocation_dir,
    );
    push_flag(
        &mut out,
        args.require_viogpu3d_readiness,
        "--require-viogpu3d-readiness",
    );
    push_flag(&mut out, args.daily, "--daily");
    push_flag(&mut out, args.release, "--release");
    push_flag(&mut out, args.skip_build, "--skip-build");
    push_flag(&mut out, args.print_policy, "--print-policy");

    Ok(out)
}
