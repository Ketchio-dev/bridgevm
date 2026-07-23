//! The Apple VZ error enums and the blocker-summary formatter they embed.

use crate::*;
use bridgevm_config::BootMode;
use bridgevm_config::VmMode;
use bridgevm_network::NetworkPlanError;
use std::path::PathBuf;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum AppleVzError {
    #[error("Apple VZ planner only supports Fast Mode manifests, got {0}")]
    UnsupportedMode(VmMode),
    #[error("Apple VZ launch requires guest arch arm64/aarch64, got {0}")]
    UnsupportedGuestArch(String),
    #[error("Apple VZ launch requires backend preferred apple-vz or unset, got {0}")]
    UnsupportedPreferredBackend(String),
    #[error("Apple VZ launch requires nat networking, got {0}")]
    UnsupportedNetworkMode(String),
    #[error("Apple VZ network plan rejected: {0}")]
    NetworkPlan(#[from] NetworkPlanError),
    #[error("Apple VZ launch requires primary disk format raw/qcow2, got {0}")]
    UnsupportedPrimaryDiskFormat(String),
    #[error("Apple VZ launch does not support guest OS {0}")]
    UnsupportedGuestOs(String),
    #[error("Apple VZ boot mode {mode} is not valid for guest OS {guest_os}")]
    InvalidBootModeForGuest { guest_os: String, mode: BootMode },
    #[error("Apple VZ boot mode {mode} requires {field}")]
    MissingBootInput { mode: BootMode, field: &'static str },
    #[error("Apple VZ boot input {field} cannot be empty")]
    EmptyBootInput { field: &'static str },
    #[error("Apple VZ boot mode {mode} cannot use {field}")]
    UnsupportedBootInput { mode: BootMode, field: &'static str },
}

#[derive(Debug, Error)]
pub enum AppleVzLaunchSpecArtifactError {
    #[error("failed to create Fast Mode launch spec directory {path}: {source}")]
    CreateDirectory {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to serialize Fast Mode launch spec: {0}")]
    Serialize(#[from] serde_json::Error),
    #[error("failed to read Fast Mode launch spec {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Fast Mode launch spec {path} exceeds the {maximum}-byte limit")]
    TooLarge { path: PathBuf, maximum: u64 },
    #[error("failed to deserialize Fast Mode launch spec {path}: {source}")]
    Deserialize {
        path: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to write Fast Mode launch spec {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
}

#[derive(Debug, Error)]
pub enum AppleVzLaunchError {
    #[error("Fast Mode launch readiness failed: {}", launch_blocker_summary(.blockers))]
    NotReady {
        blockers: Vec<AppleVzReadinessBlocker>,
    },
    #[error("{message}")]
    Unsupported {
        message: String,
        handoff: Box<AppleVzLaunchHandoff>,
    },
    #[error("failed to serialize Apple VZ launch handoff for {program}: {source}")]
    LauncherSerialize {
        program: PathBuf,
        #[source]
        source: serde_json::Error,
    },
    #[error("failed to spawn Apple VZ launcher {program}: {source}")]
    LauncherSpawn {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to write Apple VZ launch handoff to {program}: {source}")]
    LauncherWrite {
        program: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("Apple VZ launcher {program} timed out after {seconds} seconds")]
    LauncherTimeout { program: PathBuf, seconds: u64 },
    #[error("Apple VZ launcher {program} {stream} exceeded the {maximum}-byte limit")]
    LauncherOutputTooLarge {
        program: PathBuf,
        stream: &'static str,
        maximum: usize,
    },
    #[error("Apple VZ launcher {program} failed with status {status}: {output}")]
    LauncherFailed {
        program: PathBuf,
        status: String,
        stdout: String,
        stderr: String,
        output: String,
    },
}

pub(crate) fn launch_blocker_summary(blockers: &[AppleVzReadinessBlocker]) -> String {
    if blockers.is_empty() {
        return "unknown blocker".to_string();
    }
    blockers
        .iter()
        .map(|blocker| match (&blocker.path, &blocker.capability) {
            (Some(path), _) => format!("{}: {} ({path})", blocker.code, blocker.message),
            (None, Some(capability)) => {
                format!("{}: {} ({capability})", blocker.code, blocker.message)
            }
            (None, None) => format!("{}: {}", blocker.code, blocker.message),
        })
        .collect::<Vec<_>>()
        .join("; ")
}
