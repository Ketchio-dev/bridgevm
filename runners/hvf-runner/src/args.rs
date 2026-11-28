//! Command-line arguments and dispatch for hvf-runner.

use anyhow::Result;
use clap::Parser;
use std::path::PathBuf;

#[cfg(debug_assertions)]
#[path = "legacy_launch.rs"]
mod legacy_launch;
#[cfg(debug_assertions)]
pub(crate) use legacy_launch::*;
#[cfg(not(debug_assertions))]
#[path = "release_exec_policy.rs"]
mod release_exec_policy;

#[derive(Debug, Parser)]
#[command(
    name = "hvf-runner",
    about = "BridgeVM Apple Hypervisor.framework runner"
)]
pub(crate) struct Args {
    #[arg(long, value_name = "PATH")]
    pub(crate) disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) target: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) placeholder_nsid1: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) writable_disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) vars: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub(crate) evidence_dir: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) repo_root: Option<PathBuf>,
    #[arg(long)]
    pub(crate) watchdog_ms: Option<u64>,
    #[arg(long, requires = "launch", conflicts_with = "watchdog_ms")]
    pub(crate) no_watchdog: bool,
    #[arg(long)]
    pub(crate) max_reboots: Option<u32>,
    #[arg(long)]
    pub(crate) ram_mib: Option<u32>,
    #[arg(long)]
    pub(crate) smp_cpus: Option<u8>,
    #[arg(long)]
    pub(crate) boot_timer: bool,
    #[arg(long)]
    pub(crate) boot_timer_ramfb_ms: Option<u32>,
    #[arg(long)]
    pub(crate) boot_timer_desktop_checksum64: Option<String>,
    #[arg(long)]
    pub(crate) boot_timer_desktop_agent: bool,
    #[arg(long)]
    pub(crate) shutdown_after_agent_ready: bool,
    #[arg(long)]
    pub(crate) host_pause_resume_proof_ms: Option<u32>,
    #[arg(long, value_name = "PATH")]
    pub(crate) agent_service_control: Option<PathBuf>,
    #[arg(long, value_name = "COMMAND")]
    pub(crate) agent_service_command: Option<String>,
    #[arg(long)]
    pub(crate) agent_clipboard_sync: bool,
    #[arg(long, value_name = "DIR")]
    pub(crate) agent_share_host: Option<PathBuf>,
    #[arg(long, value_name = "DIR")]
    pub(crate) agent_share_guest: Option<String>,
    #[arg(long)]
    pub(crate) agent_share_ms: Option<u64>,
    #[arg(long)]
    pub(crate) agent_share_max_kb: Option<u64>,
    #[arg(long)]
    pub(crate) enable_xhci: bool,
    #[arg(long)]
    pub(crate) virtio_net: bool,
    /// An inbound TCP port forward of the NAT, `HOST:GUEST` (host 127.0.0.1
    /// only); repeatable (`BRIDGEVM_NET_FORWARD`).
    #[arg(long = "net-forward", value_name = "HOST:GUEST", value_parser = parse_net_forward)]
    pub(crate) net_forwards: Vec<(u16, u16)>,
    #[arg(long)]
    pub(crate) nvme_buffered_io: bool,
    /// The user's opt-in: every guest flush waits for F_FULLFSYNC (`BRIDGEVM_NVME_FLUSH=durable`).
    #[arg(long)]
    pub(crate) nvme_durable_flush: bool,
    /// The user's Windows display resolution, `WxH` (`BRIDGEVM_GUEST_RESOLUTION`).
    #[arg(long, value_name = "WxH", value_parser = clap::builder::NonEmptyStringValueParser::new())]
    pub(crate) guest_resolution: Option<String>,
    /// The 2D virtio-gpu display device for a guest with the signed virtio-win
    /// display driver (`BRIDGEVM_VIRTIO_GPU=1`); `--virtio-gpu-3d` wins.
    #[arg(long)]
    pub(crate) virtio_gpu: bool,
    #[arg(long)]
    pub(crate) virtio_gpu_3d: bool,
    #[arg(long)]
    pub(crate) virtio_gpu_device_id: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) gpu_trace: Option<PathBuf>,
    #[arg(long)]
    pub(crate) gpu_trace_protocol: Option<String>,
    #[arg(long)]
    pub(crate) require_gpu_trace_gate: bool,
    #[arg(long, value_name = "DIR")]
    pub(crate) viogpu3d_dir: Option<PathBuf>,
    #[arg(long)]
    pub(crate) require_viogpu3d_readiness: bool,
    #[arg(long)]
    pub(crate) daily: bool,
    #[arg(long)]
    pub(crate) release: bool,
    #[arg(long)]
    pub(crate) skip_build: bool,
    #[arg(long)]
    pub(crate) print_policy: bool,
    #[arg(long)]
    pub(crate) launch: bool,
    #[command(flatten)]
    pub(crate) typed: crate::launch_spec::LaunchSpecArgs,
    /// Reset receipt path for --launch-spec (default: in the temp dir).
    #[arg(long, value_name = "PATH")]
    pub(crate) supervise_receipt: Option<String>,
    /// Reset-cycle budget for --launch-spec.
    #[arg(long, value_name = "N", default_value_t = 100)]
    pub(crate) supervise_max_cycles: u32,
}

pub(crate) fn run() -> Result<()> {
    let args = Args::parse();

    #[cfg(not(debug_assertions))]
    release_exec_policy::validate(&args)?;

    if let Some(spec) = &args.typed.launch_spec {
        return crate::launch_spec::run_launch_spec(spec, &args);
    }

    #[cfg(debug_assertions)]
    if args.launch {
        return launch_installed_windows(&args);
    }

    println!("hvf-runner: pass --launch-spec <spec.json> (the app's launch path)");
    #[cfg(debug_assertions)]
    println!(
        "Run: hvf-runner --launch --target <installed-windows.raw> --vars <vars.fd> --evidence-dir <dir> [--daily|--smp-cpus N|--boot-timer|--print-policy]"
    );
    Ok(())
}

fn parse_net_forward(value: &str) -> Result<(u16, u16), String> {
    let ports = value.split_once(':').and_then(|(host, guest)| {
        Some((
            host.parse().ok()?,
            guest.parse().ok().filter(|&port: &u16| port != 0)?,
        ))
    });
    ports.ok_or_else(|| format!("'{value}' is not HOST:GUEST port numbers"))
}
