use super::*;
use clap::Args;
use std::path::{Component, Path};

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
    allowed(value, &[2_048, 4_096, 6_144, 8_192, 12_288, 16_384, 24_576, 32_768], "memory MiB")
}

fn allowed<T>(value: &str, choices: &[T], label: &str) -> Result<T, String>
where T: std::str::FromStr + Copy + PartialEq {
    let parsed = value.parse().map_err(|_| format!("invalid {label}: {value}"))?;
    choices.contains(&parsed).then_some(parsed)
        .ok_or_else(|| format!("unsupported {label}: {value}"))
}
