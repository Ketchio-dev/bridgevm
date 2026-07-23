//! Split out of args.rs by responsibility.

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

#[derive(Debug, Subcommand)]
pub(crate) enum StoreCommand {
    Doctor,
}

#[derive(Debug, Subcommand)]
pub(crate) enum ResourcesCommand {
    /// Re-evaluate a running Fast Mode VM's resource policy for the current
    /// power state and foreground/background visibility.
    Reapply(RuntimeResourcesArgs),
}

#[derive(Debug, Subcommand)]
pub(crate) enum RuntimeControlCommand {
    /// Query the live Apple VZ display process over its recorded control socket.
    Status(VmNameArgs),
    /// Ask the live Apple VZ display process to stop gracefully.
    Stop(VmNameArgs),
    /// Fetch the latest runtime resource policy visible to the display process.
    Policy(VmNameArgs),
    /// Summarize the display pacing view derived from the live runtime policy.
    Pacing(VmNameArgs),
    /// Re-evaluate runtime resources and ask any live display helper to read
    /// the refreshed policy.
    Reapply(RuntimeResourcesArgs),
}

#[derive(Debug, Parser)]
pub(crate) struct RuntimeResourcesArgs {
    pub(crate) name: String,
    #[arg(long, value_enum, default_value_t = RuntimeResourceVisibilityChoice::Foreground)]
    pub(crate) visibility: RuntimeResourceVisibilityChoice,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub(crate) enum RuntimeResourceVisibilityChoice {
    Foreground,
    Background,
}

impl From<RuntimeResourceVisibilityChoice> for RuntimeResourceVisibility {
    fn from(value: RuntimeResourceVisibilityChoice) -> Self {
        match value {
            RuntimeResourceVisibilityChoice::Foreground => RuntimeResourceVisibility::Foreground,
            RuntimeResourceVisibilityChoice::Background => RuntimeResourceVisibility::Background,
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct CreateArgs {
    pub(crate) name: String,
    #[arg(long, value_name = "ID")]
    pub(crate) template: Option<String>,
    #[arg(long)]
    pub(crate) os: Option<String>,
    #[arg(long)]
    pub(crate) version: Option<String>,
    #[arg(long)]
    pub(crate) arch: Option<String>,
    #[arg(long, value_enum, default_value_t = ModeChoice::Auto)]
    pub(crate) mode: ModeChoice,
    #[arg(long)]
    pub(crate) disk: Option<String>,
    #[arg(long, value_enum)]
    pub(crate) disk_format: Option<DiskFormatChoice>,
    #[arg(long, value_enum)]
    pub(crate) boot_mode: Option<BootModeChoice>,
    #[arg(long, value_name = "PATH")]
    pub(crate) installer_image: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) kernel_path: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) initrd_path: Option<String>,
    #[arg(long, value_name = "TEXT")]
    pub(crate) kernel_command_line: Option<String>,
    #[arg(long, value_name = "PATH")]
    pub(crate) macos_restore_image: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct VmNameArgs {
    pub(crate) name: String,
}

#[derive(Debug, Parser)]
pub(crate) struct DisplayArgs {
    pub(crate) name: String,
    #[arg(long, value_name = "PX")]
    pub(crate) width: Option<u32>,
    #[arg(long, value_name = "PX")]
    pub(crate) height: Option<u32>,
}

impl DisplayArgs {
    pub(crate) fn display_size(&self) -> Result<Option<(u32, u32)>> {
        match (self.width, self.height) {
            (Some(width), Some(height)) if width > 0 && height > 0 => Ok(Some((width, height))),
            (Some(_), Some(_)) => bail!("--width and --height must be positive integers"),
            (None, None) => Ok(None),
            _ => bail!("--width and --height must be provided together"),
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct ReadinessArgs {
    pub(crate) name: String,
    #[arg(long, value_name = "DIR")]
    pub(crate) live_evidence: Option<PathBuf>,
    #[arg(long)]
    pub(crate) record_live_evidence: bool,
    #[arg(long)]
    pub(crate) clear_live_evidence: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct DeleteArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) metadata_only: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct RunArgs {
    pub(crate) name: String,
    #[arg(long)]
    pub(crate) spawn: bool,
}

#[derive(Debug, Parser)]
pub(crate) struct LifecyclePlanArgs {
    pub(crate) name: String,
    #[arg(long, value_enum)]
    pub(crate) action: LifecycleActionChoice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
pub(crate) enum LifecycleActionChoice {
    Suspend,
    Resume,
}

impl From<LifecycleActionChoice> for LifecycleAction {
    fn from(value: LifecycleActionChoice) -> Self {
        match value {
            LifecycleActionChoice::Suspend => LifecycleAction::Suspend,
            LifecycleActionChoice::Resume => LifecycleAction::Resume,
        }
    }
}

#[derive(Debug, Parser)]
pub(crate) struct ExportArgs {
    pub(crate) name: String,
    #[arg(long, value_name = "PATH")]
    pub(crate) output: PathBuf,
}

#[derive(Debug, Parser)]
pub(crate) struct ImportArgs {
    pub(crate) input: PathBuf,
    #[arg(long, value_name = "NAME")]
    pub(crate) name: Option<String>,
}

#[derive(Debug, Parser)]
pub(crate) struct CloneArgs {
    pub(crate) name: String,
    pub(crate) new_name: String,
    #[arg(long)]
    pub(crate) linked: bool,
}
