//! Arguments of the virtio-gpu 3D preflight and trace report.

use crate::*;

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
