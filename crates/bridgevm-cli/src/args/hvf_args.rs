//! Own-HVF argument definitions: the reports the installed-boot wrapper runs.

use crate::*;

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
