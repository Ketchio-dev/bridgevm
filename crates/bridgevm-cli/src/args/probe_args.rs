//! Split out of args.rs by responsibility.

use crate::*;

#[derive(Debug, Parser)]
pub(crate) struct HvfInterruptTimerProbeArgs {
    #[arg(long)]
    pub(crate) allow_interrupt_timer: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVtimerExitProbeArgs {
    #[arg(long)]
    pub(crate) allow_vtimer_exit: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMemoryMapProbeArgs {
    #[arg(long)]
    pub(crate) allow_map: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfGuestEntryProbeArgs {
    #[arg(long)]
    pub(crate) allow_entry: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfGuestExitLoopProbeArgs {
    #[arg(long)]
    pub(crate) allow_loop: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioReadProbeArgs {
    #[arg(long)]
    pub(crate) allow_mmio: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioReadEmulationProbeArgs {
    #[arg(long)]
    pub(crate) allow_emulate: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioWriteEmulationProbeArgs {
    #[arg(long)]
    pub(crate) allow_emulate: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioSerialDeviceProbeArgs {
    #[arg(long)]
    pub(crate) allow_device: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioRtcDeviceProbeArgs {
    #[arg(long)]
    pub(crate) allow_device: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioBlockDeviceProbeArgs {
    #[arg(long)]
    pub(crate) allow_device: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfMmioBlockQueueProbeArgs {
    #[arg(long)]
    pub(crate) allow_device: bool,
    #[arg(long, value_name = "PATH")]
    pub(crate) disk: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) iso: Option<PathBuf>,
    #[arg(long, value_name = "PATH")]
    pub(crate) writable_disk: Option<PathBuf>,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVirtioBlockFileBackingProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) disk: PathBuf,
}

#[derive(Debug, Parser)]
pub(crate) struct HvfVirtioBlockIsoBackingProbeArgs {
    #[arg(long, value_name = "PATH")]
    pub(crate) iso: PathBuf,
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
