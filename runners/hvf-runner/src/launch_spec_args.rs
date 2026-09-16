//! Typed launch options, separated from unrelated probe arguments.
use std::path::PathBuf;
#[derive(Debug, clap::Args)]
pub(crate) struct LaunchSpecArgs {
    /// Path to a versioned typed launch manifest, or `-` for stdin. Validated
    /// by bridgevm-hvf-runtime before anything runs; in a release build the
    /// manifest may not point into a source repository.
    #[arg(long, value_name = "PATH|-")]
    pub(crate) launch_spec: Option<String>,
    /// Helper binary for --launch-spec. When present the manifest is not
    /// just validated and leased but RUN: helper generations under the
    /// reset-cycle supervisor, env_clear allowlist, no shell.
    #[arg(long, value_name = "PATH", requires = "launch_spec")]
    pub(crate) helper: Option<PathBuf>,
    /// Firmware code image for --launch-spec --helper.
    #[arg(long, value_name = "PATH", requires = "helper")]
    pub(crate) helper_firmware: Option<PathBuf>,
    /// Agent console control file for --launch-spec --helper: the guest
    /// runs the resident agent service and the supervisor appends commands
    /// here (this is how a guest reset is requested through the typed path).
    #[arg(long, value_name = "PATH", requires = "helper")]
    pub(crate) helper_agent_control: Option<PathBuf>,
    /// Evidence directory for --launch-spec --helper: enables the app-facing
    /// device surfaces (ramfb, display export, xHCI input, GPU trace) with
    /// the same env contract as the wrapper script.
    #[arg(long, value_name = "DIR", requires = "helper")]
    pub(crate) helper_evidence_dir: Option<PathBuf>,
    /// vTPM state directory for --launch-spec --helper: the supervisor runs
    /// one swtpm across every helper generation (state survives resets).
    #[arg(long, value_name = "DIR", requires = "helper")]
    pub(crate) helper_vtpm_state: Option<PathBuf>,
    /// Read the vTPM state key (raw AES-256 bytes) from stdin before the
    /// first generation. The key goes to swtpm over its fd 0 and nowhere
    /// else -- matching the wrapper's --swtpm-key-stdin contract.
    #[arg(long, requires = "helper_vtpm_state")]
    pub(crate) helper_vtpm_key_stdin: bool,
    /// Intel HDA audio through CoreAudio for --launch-spec --helper.
    #[arg(long, requires = "helper")]
    pub(crate) helper_hda: bool,
    /// swtpm binary for --helper-vtpm-state (the app passes its
    /// signature-validated choice; default is the homebrew install).
    #[arg(long, value_name = "PATH", requires = "helper_vtpm_state")]
    pub(crate) helper_swtpm_bin: Option<PathBuf>,
    #[arg(long, requires_all = ["helper", "helper_evidence_dir"], conflicts_with = "helper_vtpm_key_stdin")]
    pub(crate) owned_runtime_stdio: bool,
}
