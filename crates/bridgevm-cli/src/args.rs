//! Command-line arguments: the top-level commands and the own-HVF reports the
//! installed-boot wrapper runs. The native app commands are in `app.rs`.

use crate::*;

mod app;
pub(crate) use app::*;

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm",
    about = "BridgeVM CLI for the native app library and local HVF tools",
    after_help = "Native app inventory (no app window or guest launch):
  bridgevm app list --json
  bridgevm app inspect VM_ID

Own-HVF reports (no guest launch):
  bridgevm hvf virtio-gpu-trace-report --trace PATH
  bridgevm hvf title-gate-report --help

Use COMMAND --help for details."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// Create and manage VMs in the native macOS app library.
    App(AppArgs),
    #[command(
        subcommand,
        about = "Own-HVF local-only GPU reports",
        long_about = "Own-HVF local-only GPU reports.

These commands read trace and log files and run the synthetic 3D host preflight; they do not create a VM or manage native app VM registrations."
    )]
    Hvf(HvfCommand),
}

#[derive(Debug, Subcommand)]
pub(crate) enum HvfCommand {
    /// Exercise the synthetic virtio-gpu 3D host-visible blob map/submit/fence path.
    #[command(
        name = "virtio-gpu-3d-host-preflight",
        alias = "virtio-gpu3d-host-preflight"
    )]
    VirtioGpu3dHostPreflight(HvfVirtioGpu3dHostPreflightArgs),
    /// Summarize a BridgeVM HVF virtio-gpu JSONL trace and optionally enforce the P3 gate.
    VirtioGpuTraceReport(HvfVirtioGpuTraceReportArgs),
    /// Validate one or more real-title logs against versioned manifests and a GPU trace.
    TitleGateReport(HvfTitleGateReportArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVirtioGpu3dHostPreflightArgs {
    #[arg(long, value_enum, default_value_t = VirtioGpu3dHostPreflightProtocolChoice::Venus)]
    pub(crate) protocol: VirtioGpu3dHostPreflightProtocolChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum VirtioGpu3dHostPreflightProtocolChoice {
    Venus,
    Virgl,
}

impl From<VirtioGpu3dHostPreflightProtocolChoice> for VirtioGpu3dHostPreflightProtocol {
    fn from(value: VirtioGpu3dHostPreflightProtocolChoice) -> Self {
        match value {
            VirtioGpu3dHostPreflightProtocolChoice::Venus => Self::Venus,
            VirtioGpu3dHostPreflightProtocolChoice::Virgl => Self::Virgl,
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVirtioGpuTraceReportArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) trace: PathBuf,
    #[arg(long, value_enum, default_value_t = VirtioGpuTraceProtocolChoice::Auto)]
    pub(crate) protocol: VirtioGpuTraceProtocolChoice,
    #[arg(long)]
    pub(crate) require_p3_gate: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfTitleGateReportArgs {
    /// Versioned title manifest. Repeat once per title in the run.
    #[arg(long = "title-manifest", value_name = "PATH", required = true)]
    pub(crate) manifests: Vec<PathBuf>,
    /// Directory containing guest title logs named by each manifest.
    #[arg(long, value_name = "DIR")]
    pub(crate) guest_logs: PathBuf,
    /// Virtio-gpu JSONL trace captured during this run.
    #[arg(long, value_name = "PATH")]
    pub(crate) trace: PathBuf,
    /// Optional JSON object mapping title id to its pre-run guest-log SHA-256
    /// (or the literal string "missing").
    #[arg(long, value_name = "PATH")]
    pub(crate) pre_run_state: Option<PathBuf>,
    /// Write the machine-readable aggregate report to this path.
    #[arg(long, value_name = "PATH")]
    pub(crate) json_output: Option<PathBuf>,
    /// Exit non-zero unless every title gate passes.
    #[arg(long)]
    pub(crate) require_title_gates: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum VirtioGpuTraceProtocolChoice {
    Auto,
    Venus,
    Virgl,
}

impl VirtioGpuTraceProtocolChoice {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Venus => "venus",

            Self::Virgl => "virgl",
        }
    }
}

#[cfg(test)]
mod tests;
