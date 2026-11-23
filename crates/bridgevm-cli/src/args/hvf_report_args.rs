//! Argument types for the `hvf` report commands.

use crate::*;

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
