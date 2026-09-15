//! Split out of args.rs by responsibility.

pub(crate) use super::hvf_args::*;
use crate::*;

#[derive(Debug, Parser)]
#[command(
    name = "bridgevm",
    about = "BridgeVM developer CLI for legacy stores and local HVF tools",
    after_help = "Legacy VM commands use manifest.yaml bundles. Native macOS app registrations use vm.json; these formats are not interchangeable.

Own-HVF queries (no guest launch):
  bridgevm hvf host-capabilities
  bridgevm hvf windows-plan
  bridgevm hvf machine-plan --memory-gib 6 --vcpus 4

Legacy store inspection:
  bridgevm --store PATH list

Use COMMAND --help for effects and explicit probe opt-ins."
)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Command,
    /// Local legacy VmStore root (manifest.yaml bundles); ignored with --socket.
    #[arg(long, global = true, value_name = "PATH")]
    pub(crate) store: Option<PathBuf>,
    /// Use bridgevmd for supported legacy commands; not supported by hvf.
    #[arg(long, global = true, value_name = "SOCKET")]
    pub(crate) socket: Option<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub(crate) enum Command {
    /// List VM bundles in the legacy store.
    List,
    /// List legacy-store boot templates.
    Templates,
    /// Create a legacy manifest.yaml VM bundle.
    Create(CreateArgs),
    /// Show recorded legacy VM metadata and state.
    Status(VmNameArgs),
    /// Record running metadata only; does not launch a guest.
    Start(VmNameArgs),
    /// Stop the recorded backend and update legacy state.
    Stop(VmNameArgs),
    /// Stop the backend, then record running metadata; does not relaunch.
    Restart(VmNameArgs),
    /// Suspend a supported legacy backend.
    Suspend(VmNameArgs),
    /// Resume a supported legacy backend.
    Resume(VmNameArgs),
    /// Boot a Fast Mode VM with an embedded graphical display window (local GUI
    /// session only). Requires BRIDGEVM_APPLE_VZ_RUNNER.
    Display(DisplayArgs),
    /// Delete a legacy VM bundle, or only its metadata with --metadata-only.
    Delete(DeleteArgs),
    /// Export a legacy VM bundle.
    Export(ExportArgs),
    /// Import a legacy VM bundle into the store.
    Import(ImportArgs),
    /// Clone a legacy VM bundle.
    Clone(CloneArgs),
    /// Create a diagnostic bundle for a legacy VM.
    Diagnostics(DiagnosticsCommand),
    /// Read recorded backend logs for a legacy VM.
    Logs(LogsCommand),
    /// Record or inspect performance metadata.
    Performance(PerformanceCommand),
    /// Inspect or repair legacy VM metadata.
    Metadata(MetadataCommand),
    /// Manage legacy-store snapshots.
    Snapshot(SnapshotCommand),
    /// Inspect or modify a legacy VM disk.
    Disk(DiskCommand),
    /// Manage legacy VM port-forward metadata.
    Port(PortCommand),
    /// Show a legacy VM network plan.
    NetworkPlan(VmNameArgs),
    /// Manage legacy VM shared-folder metadata.
    Share(ShareCommand),
    /// Plan, import, download or verify boot media.
    Media(MediaCommand),
    /// Inspect guest tools or dispatch supported commands through bridgevmd.
    GuestTools(GuestToolsCommand),
    #[command(subcommand)]
    Resources(ResourcesCommand),
    #[command(subcommand)]
    RuntimeControl(RuntimeControlCommand),
    /// Show the Compatibility Engine command line.
    QemuArgs(VmNameArgs),
    /// Prepare disk and runner metadata without starting a guest.
    PrepareRun(VmNameArgs),
    /// Inspect the planned boot media for a legacy VM.
    BootMedia(VmNameArgs),
    /// Print an SSH connection plan; does not start an SSH session.
    Ssh(SshArgs),
    /// Print a forwarded-port URL plan; does not open a browser.
    Open(OpenArgs),
    /// Prepare a legacy backend dry run by default; --spawn requests execution.
    #[command(
        long_about = "Prepare a legacy backend dry run by default; --spawn requests execution.

A dry run still prepares disk and runner metadata. --spawn requests an eligible configured backend and can be refused by readiness checks. This command operates on legacy manifest.yaml bundles, not native app vm.json registrations."
    )]
    Run(RunArgs),
    /// Inspect legacy VM readiness; evidence flags can update recorded evidence.
    Readiness(ReadinessArgs),
    /// Inspect a legacy suspend or resume plan.
    LifecyclePlan(LifecyclePlanArgs),
    /// Print the recorded Compatibility Engine QMP socket path.
    QmpSocket(VmNameArgs),
    /// Query the Compatibility Engine QMP status.
    QmpStatus(VmNameArgs),
    /// Pause Compatibility Engine guest execution through QMP.
    QmpStop(VmNameArgs),
    /// Continue Compatibility Engine guest execution through QMP.
    QmpCont(VmNameArgs),
    /// Inspect recorded runner metadata and available runtime status.
    RunnerStatus(VmNameArgs),
    /// Explain the current and target engine choices for a guest.
    Recommend(GuestArgs),
    #[command(
        subcommand,
        about = "Own-HVF local-only queries and opt-in probes",
        long_about = "Own-HVF local-only queries and opt-in probes.

Host-capabilities and plan commands inspect metadata. Probes may access files or create a VM when their documented opt-ins are supplied. These commands do not manage native app VM registrations. Omit --socket."
    )]
    Hvf(HvfCommand),
    #[command(subcommand)]
    Store(StoreCommand),
    /// Inspect the legacy store and host tooling; not a guest boot test.
    Doctor,
}
