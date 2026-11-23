use super::*;
use clap::Args;
use operand::reject_option_like_operands;
use std::ffi::OsString;
use std::path::PathBuf;
use std::path::{Component, Path};

#[path = "app_operand.rs"]
mod operand;

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
    /// Saved guest memory in MiB.
    #[arg(long, default_value_t = 6144, value_parser = memory_mib)]
    pub(crate) memory_mib: u32,
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
#[rustfmt::skip]
#[derive(Debug, Subcommand)] #[command(mut_subcommands = reject_option_like_operands)]
pub(crate) enum AppCommand {
    #[command(flatten)]
    Query(AppQueryCommand), #[command(about = "Diagnose native app discovery and CLI protocol compatibility without opening the app.")] Doctor,
    #[command(about = "Create a saved installation-pending own-HVF Windows VM; does not install or boot it.", after_help = include_str!("app_windows_setup_help.txt"))] CreateWindows(AppCreateWindowsArgs),
    #[command(about = "Start a saved own-HVF VM and wait for confirmed initial helper startup.")] Start { id: String },
    #[command(about = "Stop an app-owned runtime and wait for confirmed owned-process cleanup.")] Stop { id: String },
    #[command(about = "Start or reuse the pending request already saved with this VM; ISO paths and secrets are not accepted on argv.", after_help = include_str!("app_windows_setup_help.txt"))] Install { id: String }, #[command(about = "Read retained progress for the saved pending installation; secrets are not accepted on argv.")] InstallStatus { id: String }, #[command(about = "Request cancellation of the retained installation; secrets are not accepted on argv.")] InstallCancel { id: String }, #[command(about = "Create the saved own-HVF VM's powered-off disk and UEFI-vars snapshot.")] SnapshotCreate { id: String }, #[command(about = "Restore the saved own-HVF VM's powered-off disk and UEFI-vars snapshot.")] SnapshotRestore { id: String }, #[command(about = "Export the currently selected powered-off disk and UEFI-vars pair to an external directory.")] SnapshotExport(AppSnapshotExportArgs),
}
