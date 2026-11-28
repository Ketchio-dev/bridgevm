//! Native app query and owned-control arguments.

use crate::*;
use clap::{Arg, Args, Command};
use std::ffi::OsString;
use std::path::Component;

#[derive(Debug, Parser)]
#[command(
    after_help = "Works on the VMs in your BridgeVM library. Needs BridgeVM.app (or BridgeVMControl.app) installed; start, stop, status and install also need it open, and no window opens for them. create-windows takes one absolute ISO path and copies the ISO into the new VM; snapshot-export takes one absolute output folder. Passwords and recovery keys are never accepted. install uses the ISO and drivers saved with the VM. start returns once the VM has started; Windows may still be starting up. status reports what BridgeVM sees, not whether Windows is healthy. stop waits until the VM has shut down."
)]
pub(crate) struct AppArgs {
    #[command(subcommand)]
    pub(crate) command: AppCommand,
    /// Native VM library root; must be absolute and contain no '..' component.
    #[arg(long, global = true, value_name = "PATH", value_parser = absolute_library)]
    pub(crate) library: Option<PathBuf>,
    /// Print versioned native JSON for the selected command.
    #[arg(long, global = true)]
    pub(crate) json: bool,
}

/// Validate the native library path without opening or modifying it.
fn absolute_library(value: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("--library requires an absolute path without '..'".into());
    }
    Ok(path)
}

// Read-only native app commands retain their original syntax.
#[derive(Debug, Subcommand)]
pub(crate) enum AppQueryCommand {
    /// List the VMs in your library, and any library folders that need attention.
    List,
    /// Show one VM's saved settings, by the exact ID that list prints (Korean IDs too).
    Inspect { id: String },
    /// Check whether a VM can start, without starting it.
    Readiness { id: String },
    /// Show what BridgeVM reports about a VM; needs the app open.
    Status { id: String },
}

impl AppQueryCommand {
    pub(crate) fn into_parts(self) -> (&'static str, Option<String>) {
        match self {
            Self::List => ("list", None),
            Self::Inspect { id } => ("inspect", Some(id)),
            Self::Readiness { id } => ("readiness", Some(id)),
            Self::Status { id } => ("status", Some(id)),
        }
    }
}

#[derive(Debug, Args)]
pub(crate) struct AppCreateWindowsArgs {
    /// Display name for the new VM; its canonical ID is returned after creation.
    pub(crate) name: String,
    /// Absolute path to a readable Windows ARM installation ISO.
    #[arg(long, value_name = "PATH", value_parser = absolute_iso)]
    pub(crate) iso: PathBuf,
    /// New raw target size in GiB.
    #[arg(long, default_value_t = 64, value_parser = disk_gib)]
    pub(crate) disk_gib: u16,
    /// Saved guest memory in MiB [default: the app's choice for this Mac, as in Create: 6144, or
    /// 4096 on an 8 GiB Mac].
    #[arg(long, value_parser = memory_mib)]
    pub(crate) memory_mib: Option<u32>,
    /// Saved virtual CPU count; the app also enforces the current host limit.
    #[arg(long, default_value_t = 4, value_parser = clap::value_parser!(u16).range(1..=64))]
    pub(crate) cpus: u16,
    /// Saved display resolution.
    #[arg(long, default_value = "1440x900", value_parser = ["1280x800", "1440x900", "1920x1080", "2560x1440"])]
    pub(crate) resolution: String,
    /// Disable the default shared NAT network.
    #[arg(long)]
    pub(crate) no_network: bool,
    /// Download the pinned Microsoft-signed Red Hat virtio-win storage, serial and network
    /// drivers (about 95 MB) and seal them with the VM so `app install` can complete it.
    #[arg(long)]
    pub(crate) fetch_drivers: bool,
}

fn absolute_iso(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if !path.is_absolute() || path.components().any(|part| part == Component::ParentDir) {
        return Err("ISO path must be absolute and contain no '..' component".into());
    }
    Ok(path.to_path_buf())
}

fn disk_gib(value: &str) -> Result<u16, String> {
    allowed(value, &[64, 96, 128, 256, 512], "disk GiB")
}

fn memory_mib(value: &str) -> Result<u32, String> {
    allowed(
        value,
        &[2_048, 4_096, 6_144, 8_192, 12_288, 16_384, 24_576, 32_768],
        "memory MiB",
    )
}

fn allowed<T>(value: &str, choices: &[T], label: &str) -> Result<T, String>
where
    T: std::str::FromStr + Copy + PartialEq,
{
    let parsed = value
        .parse()
        .map_err(|_| format!("invalid {label}: {value}"))?;
    choices
        .contains(&parsed)
        .then_some(parsed)
        .ok_or_else(|| format!("unsupported {label}: {value}"))
}

#[derive(Debug, Args)]
pub(crate) struct AppSnapshotExportArgs {
    pub(crate) id: String,
    #[arg(value_name = "OUTPUT", value_parser = absolute_export)]
    pub(crate) output: PathBuf,
}

fn absolute_export(value: &str) -> std::result::Result<PathBuf, String> {
    let path = PathBuf::from(value);
    if !path.is_absolute()
        || path
            .components()
            .any(|part| part == std::path::Component::ParentDir)
    {
        return Err("OUTPUT requires an absolute path without '..'".into());
    }
    if path.parent().is_none() {
        return Err("OUTPUT must name an export directory".into());
    }
    Ok(path)
}

impl AppSnapshotExportArgs {
    pub(crate) fn into_native_arguments(self) -> Vec<OsString> {
        [
            OsString::from("--cli"),
            OsString::from("snapshot-export"),
            OsString::from(self.id),
            self.output.into_os_string(),
        ]
        .into()
    }
}

// Native query and owned-runtime command selection.
#[rustfmt::skip]
#[derive(Debug, Subcommand)] #[command(mut_subcommands = reject_option_like_operands)]
pub(crate) enum AppCommand {
    #[command(flatten)]
    Query(AppQueryCommand), #[command(about = "Diagnose native app discovery and CLI protocol compatibility without opening the app.")] Doctor,
    #[command(about = "Create a Windows VM in your library, ready to install; does not install or start it.", after_help = include_str!("app_windows_setup_help.txt"))] CreateWindows(AppCreateWindowsArgs),
    #[command(about = "Start a VM and wait until BridgeVM confirms it started.")] Start { id: String },
    #[command(about = "Stop a VM that BridgeVM started and wait until it has shut down.")] Stop { id: String },
    #[command(about = "Install Windows in a VM created for installation, from the ISO saved with it; no paths or secrets are accepted.", after_help = include_str!("app_windows_setup_help.txt"))] Install { id: String }, #[command(about = "Show the progress of the VM's Windows installation.")] InstallStatus { id: String }, #[command(about = "Cancel the VM's Windows installation.")] InstallCancel { id: String }, #[command(about = "Take a snapshot of a shut-down VM's disk and boot settings.")] SnapshotCreate { id: String }, #[command(about = "Restore a shut-down VM's disk and boot settings from its newest snapshot.")] SnapshotRestore { id: String }, #[command(about = "Copy a shut-down VM's current disk and boot settings to a folder outside the VM.")] SnapshotExport(AppSnapshotExportArgs),
}

/// Forwarded VM ID and NAME operands must not look like native options.
///
/// Native `BridgeVMControl --cli` has no `--` operand separator, so a value that
/// clap accepts after `--` (for example `--help`) would be re-read there as an
/// option. Canonical VM IDs never start with '-', so such values are refused here.
fn reject_option_like_operands(command: Command) -> Command {
    command.mut_args(guard_operand)
}

fn guard_operand(arg: Arg) -> Arg {
    if arg.is_positional() && ["id", "name"].contains(&arg.get_id().as_str()) {
        arg.value_parser(vm_operand)
    } else {
        arg
    }
}

fn vm_operand(value: &str) -> Result<String, String> {
    if value.starts_with('-') {
        return Err("VM IDs and names starting with '-' are not accepted by the CLI".into());
    }
    Ok(value.to_owned())
}
