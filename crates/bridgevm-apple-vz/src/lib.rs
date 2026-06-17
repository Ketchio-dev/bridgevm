use bridgevm_config::{Boot, BootMode, SharedFolder, VmManifest, VmMode};
use bridgevm_network::{
    plan_network, NetworkBackend, NetworkMode, NetworkPlan, NetworkPlanError, PortForwardRule,
};
use bridgevm_resource_manager::{decide_from_manifest_profile, resolve_memory, resolve_vcpu};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
};
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
    #[error("Apple VZ launcher {program} failed with status {status}: {output}")]
    LauncherFailed {
        program: PathBuf,
        status: String,
        stdout: String,
        stderr: String,
        output: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLinuxConfig {
    pub kernel_path: Option<String>,
    pub disk_path: String,
    pub memory: String,
    pub cpu: String,
    pub virtiofs: bool,
}

impl AppleVzLinuxConfig {
    pub fn automatic(disk_path: impl Into<String>) -> Self {
        Self {
            kernel_path: None,
            disk_path: disk_path.into(),
            memory: "auto".to_string(),
            cpu: "auto".to_string(),
            virtiofs: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzPlan {
    pub vm_name: String,
    pub guest_os: String,
    pub guest_arch: String,
    pub config: AppleVzLinuxConfig,
    pub launch_spec: AppleVzLaunchSpec,
    pub entropy_device: bool,
    pub balloon_device: bool,
    pub clipboard: bool,
    pub shared_folders: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLaunchSpec {
    pub vm_name: String,
    pub bundle_path: String,
    pub guest: AppleVzGuestSpec,
    pub boot: AppleVzBootSpec,
    pub disk: AppleVzDiskSpec,
    pub resources: AppleVzResourceSpec,
    pub devices: AppleVzDeviceSpec,
    pub integration: AppleVzIntegrationSpec,
    pub logs: AppleVzLogSpec,
    pub readiness: AppleVzReadinessSpec,
    /// Virtio-FS shared directories handed to the AppleVzRunner helper. The Swift
    /// side attaches each as a `VZSharedDirectory` (one `VZSingleDirectoryShare`
    /// for a single entry, a `VZMultipleDirectoryShare` for 2+); see
    /// [`build_fast_plan`] for how the manifest's approved folders are mapped.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<AppleVzShareSpec>,
}

/// Single Virtio-FS shared directory destined for the AppleVzRunner helper via a
/// repeatable `--share <tag>=<host_path>` (optionally `ro:`-prefixed) flag.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzShareSpec {
    /// Host path of the shared directory.
    pub host_path: String,
    /// Share/mount tag (the folder `name`, or a derived `"share"`/`"share-N"`).
    pub tag: String,
    /// When true, the directory is shared read-only.
    pub read_only: bool,
}

/// Encode a single share as one `--share` flag value.
///
/// Grammar (consumed verbatim by the Swift AppleVzRunner `--share` parser):
///
/// ```text
/// share-value := [ "ro:" ] tag "=" host-path
/// ```
///
/// The optional `ro:` prefix marks the share read-only. `tag` is everything up
/// to the FIRST `=`; `host-path` is the remainder. Splitting on the first `=`
/// keeps host paths that contain `=`, spaces, or commas intact, and VZ share
/// tags are validated to exclude `=`, so the boundary is unambiguous. Tags here
/// are never empty (callers derive a non-empty tag), so the value never starts
/// with a bare `=`.
pub fn encode_share_flag_value(share: &AppleVzShareSpec) -> String {
    let prefix = if share.read_only { "ro:" } else { "" };
    format!("{prefix}{}={}", share.tag, share.host_path)
}

/// Default Virtio-FS share tag, matching the AppleVzRunner Swift default
/// (`AppleVzSharedDirectorySpec` tag "share").
const DEFAULT_SHARE_TAG: &str = "share";

/// Build the Virtio-FS shares to hand to the AppleVzRunner helper.
///
/// Returns an empty `Vec` unless `integration.shared_folders` is enabled. When
/// enabled, EVERY approved `SharedFolder` is mapped to an `AppleVzShareSpec` so
/// the Swift side can attach all of them (a `VZSingleDirectoryShare` for one, a
/// `VZMultipleDirectoryShare` for 2+).
///
/// VZ requires every share tag to be unique. Each folder's tag is its `name`
/// (trimmed); empty names get the default `"share"` tag. To keep tags unique,
/// any tag that collides with an earlier one is disambiguated by appending
/// `-2`, `-3`, ... (and so on until unique), so unnamed/duplicate folders never
/// clash.
fn build_share_specs(
    shared_folders_enabled: bool,
    shared_folders: &[SharedFolder],
) -> Vec<AppleVzShareSpec> {
    if !shared_folders_enabled {
        return Vec::new();
    }
    let mut used_tags: Vec<String> = Vec::with_capacity(shared_folders.len());
    shared_folders
        .iter()
        .map(|folder| {
            let base = if folder.name.trim().is_empty() {
                DEFAULT_SHARE_TAG.to_string()
            } else {
                folder.name.clone()
            };
            let tag = unique_tag(&base, &used_tags);
            used_tags.push(tag.clone());
            AppleVzShareSpec {
                host_path: folder.host_path.clone(),
                tag,
                read_only: folder.read_only,
            }
        })
        .collect()
}

/// Derive a tag that does not collide with any already-used tag by appending a
/// numeric suffix (`-2`, `-3`, ...) until unique.
fn unique_tag(base: &str, used: &[String]) -> String {
    if !used.iter().any(|tag| tag == base) {
        return base.to_string();
    }
    let mut suffix = 2;
    loop {
        let candidate = format!("{base}-{suffix}");
        if !used.iter().any(|tag| tag == &candidate) {
            return candidate;
        }
        suffix += 1;
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLaunchHandoff {
    pub backend: String,
    pub vm_name: String,
    pub bundle_path: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub launch_spec_path: Option<String>,
    pub guest: AppleVzGuestSpec,
    pub boot_mode: BootMode,
    pub disk: AppleVzDiskSpec,
    pub resources: AppleVzResourceSpec,
    pub runner_log_path: String,
    pub serial_log_path: String,
    pub integration: AppleVzIntegrationSpec,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shares: Vec<AppleVzShareSpec>,
    pub readiness: AppleVzReadinessSpec,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLaunchAttempt {
    pub backend: String,
    pub vm_name: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stdout: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub stderr: String,
}

pub trait AppleVzLauncher {
    fn launch(
        &self,
        handoff: AppleVzLaunchHandoff,
    ) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError>;
}

#[derive(Debug, Default, Clone, Copy)]
pub struct UnsupportedAppleVzLauncher;

impl AppleVzLauncher for UnsupportedAppleVzLauncher {
    fn launch(
        &self,
        handoff: AppleVzLaunchHandoff,
    ) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError> {
        Err(AppleVzLaunchError::Unsupported {
            message: "Apple Virtualization.framework launch is not implemented yet".to_string(),
            handoff: Box::new(handoff),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AppleVzCommandLauncher {
    program: PathBuf,
    args: Vec<String>,
    env: Vec<(String, String)>,
}

impl AppleVzCommandLauncher {
    pub fn new(program: impl Into<PathBuf>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: Vec::new(),
        }
    }

    pub fn arg(mut self, arg: impl Into<String>) -> Self {
        self.args.push(arg.into());
        self
    }

    pub fn env(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.env.push((key.into(), value.into()));
        self
    }
}

impl AppleVzLauncher for AppleVzCommandLauncher {
    fn launch(
        &self,
        handoff: AppleVzLaunchHandoff,
    ) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError> {
        let input = serde_json::to_vec(&handoff).map_err(|source| {
            AppleVzLaunchError::LauncherSerialize {
                program: self.program.clone(),
                source,
            }
        })?;
        let mut child = Command::new(&self.program)
            .args(&self.args)
            .envs(self.env.iter().map(|(key, value)| (key, value)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|source| AppleVzLaunchError::LauncherSpawn {
                program: self.program.clone(),
                source,
            })?;
        let mut stdin = child.stdin.take().expect("piped stdin should be present");
        stdin
            .write_all(&input)
            .map_err(|source| AppleVzLaunchError::LauncherWrite {
                program: self.program.clone(),
                source,
            })?;
        drop(stdin);
        let output =
            child
                .wait_with_output()
                .map_err(|source| AppleVzLaunchError::LauncherSpawn {
                    program: self.program.clone(),
                    source,
                })?;
        let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        if !output.status.success() {
            return Err(AppleVzLaunchError::LauncherFailed {
                program: self.program.clone(),
                status: output.status.to_string(),
                stdout: stdout.clone(),
                stderr: stderr.clone(),
                output: format_launcher_output(&stdout, &stderr),
            });
        }
        Ok(AppleVzLaunchAttempt {
            backend: handoff.backend,
            vm_name: handoff.vm_name,
            stdout,
            stderr,
        })
    }
}

fn format_launcher_output(stdout: &str, stderr: &str) -> String {
    match (stdout.is_empty(), stderr.is_empty()) {
        (true, true) => "no output".to_string(),
        (true, false) => stderr.to_string(),
        (false, true) => format!("stdout: {stdout}"),
        (false, false) => format!("stderr: {stderr}; stdout: {stdout}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzGuestSpec {
    pub os: String,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzBootSpec {
    pub mode: BootMode,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub installer_image: Option<AppleVzPathSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel: Option<AppleVzPathSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initrd: Option<AppleVzPathSpec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kernel_command_line: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub macos_restore_image: Option<AppleVzPathSpec>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzPathSpec {
    pub path: String,
    pub exists: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzDiskSpec {
    pub path: String,
    pub format: String,
    pub read_only: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzResourceSpec {
    pub memory: String,
    pub cpu: String,
    pub display_fps_cap: String,
    pub rationale: String,
    pub balloon_device: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzDeviceSpec {
    pub entropy_device: bool,
    pub network: String,
    pub serial_log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzIntegrationSpec {
    pub clipboard: bool,
    pub dynamic_resolution: bool,
    pub shared_folders: bool,
    pub virtiofs: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzLogSpec {
    pub runner_log_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzReadinessSpec {
    pub ready: bool,
    pub blockers: Vec<AppleVzReadinessBlocker>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppleVzReadinessBlocker {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub capability: Option<String>,
}

impl AppleVzPlan {
    pub fn render_runner_words(&self) -> Vec<String> {
        let mut words = vec![
            "lightvm-runner".to_string(),
            self.vm_name.clone(),
            "--apple-vz".to_string(),
            "--disk".to_string(),
            self.config.disk_path.clone(),
            "--memory".to_string(),
            self.config.memory.clone(),
            "--cpu".to_string(),
            self.config.cpu.clone(),
        ];
        for share in &self.launch_spec.shares {
            words.push("--share".to_string());
            words.push(encode_share_flag_value(share));
        }
        words
    }

    pub fn launch_spec(&self) -> &AppleVzLaunchSpec {
        &self.launch_spec
    }
}

pub fn build_fast_plan(
    manifest: &VmManifest,
    bundle_path: &Path,
) -> Result<AppleVzPlan, AppleVzError> {
    if manifest.mode != VmMode::Fast {
        return Err(AppleVzError::UnsupportedMode(manifest.mode));
    }
    preflight_apple_vz_launch(manifest)?;

    let disk_path = resolve_bundle_path(bundle_path, &manifest.storage.primary.path);
    let disk_path = disk_path.display().to_string();
    let bundle_path = bundle_path.display().to_string();
    let runner_log_path = resolve_bundle_path(Path::new(&bundle_path), "logs/lightvm.log")
        .display()
        .to_string();
    let serial_log_path = resolve_bundle_path(Path::new(&bundle_path), "logs/serial.log")
        .display()
        .to_string();
    let balloon_device = manifest.resources.profile == "automatic";
    let resource_decision = decide_from_manifest_profile(&manifest.resources.profile);
    let memory = resolve_memory(&manifest.resources.memory, &resource_decision);
    let cpu = resolve_vcpu(&manifest.resources.cpu, &resource_decision);
    let boot = build_boot_spec(manifest, Path::new(&bundle_path))?;
    let readiness = build_readiness_spec(&boot, &disk_path, &AppleVzHostCapability::current());
    let shares = build_share_specs(
        manifest.integration.shared_folders,
        &manifest.shared_folders,
    );
    let launch_spec = AppleVzLaunchSpec {
        vm_name: manifest.name.clone(),
        bundle_path: bundle_path.clone(),
        guest: AppleVzGuestSpec {
            os: manifest.guest.os.clone(),
            arch: manifest.guest.arch.clone(),
        },
        boot,
        disk: AppleVzDiskSpec {
            path: disk_path.clone(),
            format: manifest.storage.primary.format.clone(),
            read_only: false,
        },
        resources: AppleVzResourceSpec {
            memory: memory.clone(),
            cpu: cpu.clone(),
            display_fps_cap: resource_decision.display_fps_cap.clone(),
            rationale: resource_decision.rationale.clone(),
            balloon_device,
        },
        devices: AppleVzDeviceSpec {
            entropy_device: true,
            network: manifest.network.mode.clone(),
            serial_log_path,
        },
        integration: AppleVzIntegrationSpec {
            clipboard: manifest.integration.clipboard,
            dynamic_resolution: manifest.integration.dynamic_resolution,
            shared_folders: manifest.integration.shared_folders,
            virtiofs: manifest.integration.shared_folders,
        },
        logs: AppleVzLogSpec { runner_log_path },
        shares,
        readiness,
    };

    Ok(AppleVzPlan {
        vm_name: manifest.name.clone(),
        guest_os: manifest.guest.os.clone(),
        guest_arch: manifest.guest.arch.clone(),
        config: AppleVzLinuxConfig {
            kernel_path: launch_spec
                .boot
                .kernel
                .as_ref()
                .map(|kernel| kernel.path.clone()),
            disk_path: disk_path.clone(),
            memory,
            cpu,
            virtiofs: manifest.integration.shared_folders,
        },
        launch_spec,
        entropy_device: true,
        balloon_device,
        clipboard: manifest.integration.clipboard,
        shared_folders: manifest.integration.shared_folders,
    })
}

pub fn launch_spec_path(bundle_path: &Path) -> PathBuf {
    bundle_path.join("metadata").join("apple-vz-launch.json")
}

pub fn write_launch_spec_artifact(
    bundle_path: &Path,
    spec: &AppleVzLaunchSpec,
) -> Result<PathBuf, AppleVzLaunchSpecArtifactError> {
    let path = launch_spec_path(bundle_path);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| {
            AppleVzLaunchSpecArtifactError::CreateDirectory {
                path: parent.to_path_buf(),
                source,
            }
        })?;
    }
    let json = serde_json::to_string_pretty(spec)?;
    fs::write(&path, json).map_err(|source| AppleVzLaunchSpecArtifactError::Write {
        path: path.clone(),
        source,
    })?;
    Ok(path)
}

pub fn read_launch_spec_artifact(
    path: &Path,
) -> Result<AppleVzLaunchSpec, AppleVzLaunchSpecArtifactError> {
    let bytes = fs::read(path).map_err(|source| AppleVzLaunchSpecArtifactError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_slice(&bytes).map_err(|source| AppleVzLaunchSpecArtifactError::Deserialize {
        path: path.to_path_buf(),
        source,
    })
}

pub fn build_launch_handoff(
    spec: &AppleVzLaunchSpec,
    launch_spec_path: Option<&Path>,
) -> AppleVzLaunchHandoff {
    AppleVzLaunchHandoff {
        backend: "apple-virtualization-framework".to_string(),
        vm_name: spec.vm_name.clone(),
        bundle_path: spec.bundle_path.clone(),
        launch_spec_path: launch_spec_path.map(|path| path.display().to_string()),
        guest: spec.guest.clone(),
        boot_mode: spec.boot.mode,
        disk: spec.disk.clone(),
        resources: spec.resources.clone(),
        runner_log_path: spec.logs.runner_log_path.clone(),
        serial_log_path: spec.devices.serial_log_path.clone(),
        integration: spec.integration.clone(),
        shares: spec.shares.clone(),
        readiness: spec.readiness.clone(),
    }
}

pub fn launch_with_apple_vz<L: AppleVzLauncher>(
    launcher: &L,
    handoff: AppleVzLaunchHandoff,
) -> Result<AppleVzLaunchAttempt, AppleVzLaunchError> {
    ensure_launch_handoff_ready(&handoff)?;
    launcher.launch(handoff)
}

pub fn ensure_launch_handoff_ready(
    handoff: &AppleVzLaunchHandoff,
) -> Result<(), AppleVzLaunchError> {
    if handoff.readiness.ready {
        Ok(())
    } else {
        Err(AppleVzLaunchError::NotReady {
            blockers: handoff.readiness.blockers.clone(),
        })
    }
}

fn launch_blocker_summary(blockers: &[AppleVzReadinessBlocker]) -> String {
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

fn preflight_apple_vz_launch(manifest: &VmManifest) -> Result<(), AppleVzError> {
    let guest_arch = manifest.guest.arch.to_ascii_lowercase();
    if !matches!(guest_arch.as_str(), "arm64" | "aarch64") {
        return Err(AppleVzError::UnsupportedGuestArch(
            manifest.guest.arch.clone(),
        ));
    }

    if let Some(preferred) = &manifest.backend.preferred {
        if preferred != "apple-vz" {
            return Err(AppleVzError::UnsupportedPreferredBackend(preferred.clone()));
        }
    }

    let _network_plan = apple_vz_network_plan(manifest)?;

    if !matches!(manifest.storage.primary.format.as_str(), "raw" | "qcow2") {
        return Err(AppleVzError::UnsupportedPrimaryDiskFormat(
            manifest.storage.primary.format.clone(),
        ));
    }

    let guest_os = manifest.guest.os.to_ascii_lowercase();
    if !matches!(
        guest_os.as_str(),
        "ubuntu" | "fedora" | "debian" | "linux" | "macos"
    ) {
        return Err(AppleVzError::UnsupportedGuestOs(manifest.guest.os.clone()));
    }

    validate_boot(manifest.boot.as_ref(), &guest_os)?;

    Ok(())
}

fn apple_vz_network_plan(manifest: &VmManifest) -> Result<NetworkPlan, AppleVzError> {
    let mode = NetworkMode::from_str(&manifest.network.mode)
        .map_err(|_| AppleVzError::UnsupportedNetworkMode(manifest.network.mode.clone()))?;
    let port_forwards = manifest
        .network
        .forwards
        .iter()
        .map(|forward| PortForwardRule {
            host: forward.host,
            guest: forward.guest,
        })
        .collect();
    let plan = plan_network(
        NetworkBackend::AppleVz,
        mode,
        manifest.network.hostname.clone(),
        port_forwards,
    )
    .map_err(|error| match error {
        NetworkPlanError::UnsupportedMode { mode, .. }
        | NetworkPlanError::UnsupportedPortForwarding { mode } => {
            AppleVzError::UnsupportedNetworkMode(mode.to_string())
        }
        other => AppleVzError::NetworkPlan(other),
    })?;

    if plan.mode != NetworkMode::Nat {
        return Err(AppleVzError::UnsupportedNetworkMode(plan.mode.to_string()));
    }

    Ok(plan)
}

fn validate_boot(boot: Option<&Boot>, guest_os: &str) -> Result<(), AppleVzError> {
    let Some(boot) = boot else {
        return Ok(());
    };
    let mode = boot.mode;
    for (field, value) in [
        ("installerImage", boot.installer_image.as_deref()),
        ("kernelPath", boot.kernel_path.as_deref()),
        ("initrdPath", boot.initrd_path.as_deref()),
        ("kernelCommandLine", boot.kernel_command_line.as_deref()),
        ("macosRestoreImage", boot.macos_restore_image.as_deref()),
    ] {
        if value.is_some_and(|value| value.trim().is_empty()) {
            return Err(AppleVzError::EmptyBootInput { field });
        }
    }

    let linux_guest = matches!(guest_os, "ubuntu" | "fedora" | "debian" | "linux");
    match mode {
        BootMode::ExistingDisk if boot.macos_restore_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::ExistingDisk if boot.installer_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::ExistingDisk if boot.kernel_path.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::ExistingDisk => Ok(()),
        BootMode::LinuxKernel if !linux_guest => Err(AppleVzError::InvalidBootModeForGuest {
            guest_os: guest_os.to_string(),
            mode,
        }),
        BootMode::LinuxKernel if boot.kernel_path.is_none() => {
            Err(AppleVzError::MissingBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::LinuxKernel if boot.installer_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxKernel if boot.macos_restore_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxKernel => Ok(()),
        BootMode::LinuxInstaller if !linux_guest => Err(AppleVzError::InvalidBootModeForGuest {
            guest_os: guest_os.to_string(),
            mode,
        }),
        BootMode::LinuxInstaller if boot.installer_image.is_none() => {
            Err(AppleVzError::MissingBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxInstaller if boot.kernel_path.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::LinuxInstaller if boot.macos_restore_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxInstaller => Ok(()),
        // Apple VZ (Fast Mode) cannot run Windows guests; windows-installer is a
        // Compatibility Mode (QEMU) boot mode only.
        BootMode::WindowsInstaller => Err(AppleVzError::InvalidBootModeForGuest {
            guest_os: guest_os.to_string(),
            mode,
        }),
        BootMode::MacosRestore if guest_os != "macos" => {
            Err(AppleVzError::InvalidBootModeForGuest {
                guest_os: guest_os.to_string(),
                mode,
            })
        }
        BootMode::MacosRestore if boot.macos_restore_image.is_none() => {
            Err(AppleVzError::MissingBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::MacosRestore if boot.installer_image.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::MacosRestore if boot.kernel_path.is_some() => {
            Err(AppleVzError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::MacosRestore => Ok(()),
    }
}

fn build_boot_spec(
    manifest: &VmManifest,
    bundle_path: &Path,
) -> Result<AppleVzBootSpec, AppleVzError> {
    let Some(boot) = manifest.boot.as_ref() else {
        return Ok(AppleVzBootSpec {
            mode: BootMode::ExistingDisk,
            installer_image: None,
            kernel: None,
            initrd: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
    };
    validate_boot(Some(boot), &manifest.guest.os.to_ascii_lowercase())?;
    Ok(AppleVzBootSpec {
        mode: boot.mode,
        installer_image: boot
            .installer_image
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
        kernel: boot
            .kernel_path
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
        initrd: boot
            .initrd_path
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
        kernel_command_line: boot.kernel_command_line.clone(),
        macos_restore_image: boot
            .macos_restore_image
            .as_deref()
            .map(|path| resolved_path_spec(bundle_path, path)),
    })
}

fn resolved_path_spec(bundle_path: &Path, relative_or_absolute: &str) -> AppleVzPathSpec {
    let path = resolve_bundle_path(bundle_path, relative_or_absolute);
    AppleVzPathSpec {
        exists: path.exists(),
        path: path.display().to_string(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct AppleVzHostCapability {
    os: String,
    arch: String,
}

impl AppleVzHostCapability {
    fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }

    fn is_macos(&self) -> bool {
        self.os == "macos"
    }

    fn is_apple_silicon(&self) -> bool {
        matches!(self.arch.as_str(), "aarch64" | "arm64")
    }
}

fn build_readiness_spec(
    boot: &AppleVzBootSpec,
    disk_path: &str,
    host: &AppleVzHostCapability,
) -> AppleVzReadinessSpec {
    let mut blockers = Vec::new();
    append_host_readiness_blockers(&mut blockers, host);

    let disk = Path::new(disk_path);
    if !disk.exists() {
        blockers.push(AppleVzReadinessBlocker {
            code: "missing-primary-disk".to_string(),
            message: "Primary disk is missing; prepare or create the disk before Fast Mode launch."
                .to_string(),
            path: Some(disk_path.to_string()),
            capability: None,
        });
    }

    for (code, label, media) in [
        (
            "missing-installer-image",
            "Installer image",
            boot.installer_image.as_ref(),
        ),
        ("missing-kernel", "Kernel", boot.kernel.as_ref()),
        ("missing-initrd", "Initrd", boot.initrd.as_ref()),
        (
            "missing-macos-restore-image",
            "macOS restore image",
            boot.macos_restore_image.as_ref(),
        ),
    ] {
        if let Some(media) = media {
            if !media.exists {
                blockers.push(AppleVzReadinessBlocker {
                    code: code.to_string(),
                    message: format!(
                        "{label} is missing; import, verify, or download boot media before launch."
                    ),
                    path: Some(media.path.clone()),
                    capability: None,
                });
            }
        }
    }

    AppleVzReadinessSpec {
        ready: blockers.is_empty(),
        blockers,
    }
}

fn append_host_readiness_blockers(
    blockers: &mut Vec<AppleVzReadinessBlocker>,
    host: &AppleVzHostCapability,
) {
    if !host.is_macos() {
        blockers.push(AppleVzReadinessBlocker {
            code: "unsupported-host-os".to_string(),
            message: format!(
                "Apple Virtualization Fast Mode launch requires macOS; current host reports {}.",
                host.os
            ),
            path: None,
            capability: Some("apple-virtualization-framework".to_string()),
        });
    }

    if !host.is_apple_silicon() {
        blockers.push(AppleVzReadinessBlocker {
            code: "unsupported-host-arch".to_string(),
            message: format!(
                "Apple Virtualization Fast Mode launch requires Apple Silicon; current host arch is {}.",
                host.arch
            ),
            path: None,
            capability: Some("apple-silicon".to_string()),
        });
    }
}

fn resolve_bundle_path(bundle_path: &Path, relative_or_absolute: &str) -> PathBuf {
    let path = PathBuf::from(relative_or_absolute);
    if path.is_absolute() {
        path
    } else {
        bundle_path.join(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bridgevm_config::{Boot, BootMode, Guest, SharedFolder, VmManifest, VmMode};

    fn shared_folder(name: &str, host_path: &str, read_only: bool) -> SharedFolder {
        SharedFolder {
            name: name.to_string(),
            host_path: host_path.to_string(),
            read_only,
            host_path_token: None,
        }
    }

    fn valid_fast_manifest() -> VmManifest {
        VmManifest::new(
            "Ubuntu Fast",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        )
    }

    #[test]
    fn builds_fast_mode_plan() {
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        assert_eq!(plan.vm_name, "Ubuntu Fast");
        assert!(plan.config.disk_path.ends_with("disks/root.qcow2"));
        assert_eq!(plan.launch_spec.vm_name, "Ubuntu Fast");
        assert_eq!(plan.launch_spec.guest.os, "ubuntu");
        assert_eq!(plan.launch_spec.guest.arch, "arm64");
        assert_eq!(plan.launch_spec.boot.mode, BootMode::ExistingDisk);
        assert_eq!(plan.launch_spec.disk.format, "qcow2");
        assert!(plan.launch_spec.disk.path.ends_with("disks/root.qcow2"));
        assert_eq!(plan.launch_spec.resources.memory, "4096");
        assert_eq!(plan.launch_spec.resources.cpu, "2");
        assert_eq!(plan.launch_spec.resources.display_fps_cap, "adaptive");
        assert_eq!(
            plan.launch_spec.resources.rationale,
            "Automatic balanced policy."
        );
        assert!(plan.launch_spec.resources.balloon_device);
        assert_eq!(plan.launch_spec.devices.network, "nat");
        assert!(plan
            .launch_spec
            .devices
            .serial_log_path
            .ends_with("logs/serial.log"));
        assert!(plan.launch_spec.integration.clipboard);
        assert!(plan.launch_spec.integration.dynamic_resolution);
        assert!(plan.launch_spec.integration.shared_folders);
        assert!(plan.launch_spec.integration.virtiofs);
        assert!(plan
            .launch_spec
            .logs
            .runner_log_path
            .ends_with("logs/lightvm.log"));
        assert!(!plan.launch_spec.readiness.ready);
        assert!(plan
            .launch_spec
            .readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-primary-disk"));
        assert_eq!(
            plan.render_runner_words().first().unwrap(),
            "lightvm-runner"
        );
    }

    #[test]
    fn resource_profile_applies_to_auto_values_and_preserves_manual_overrides() {
        let mut manifest = valid_fast_manifest();
        manifest.resources.profile = "performance".to_string();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/perf.vmbridge")).unwrap();
        assert_eq!(plan.launch_spec.resources.memory, "6144");
        assert_eq!(plan.launch_spec.resources.cpu, "4");
        assert_eq!(plan.launch_spec.resources.display_fps_cap, "60");
        assert_eq!(plan.config.memory, "6144");
        assert_eq!(plan.config.cpu, "4");

        manifest.resources.memory = "8192".to_string();
        manifest.resources.cpu = "6".to_string();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/manual.vmbridge")).unwrap();
        assert_eq!(plan.launch_spec.resources.memory, "8192");
        assert_eq!(plan.launch_spec.resources.cpu, "6");
        assert_eq!(plan.config.memory, "8192");
        assert_eq!(plan.config.cpu, "6");
    }

    #[test]
    fn launch_spec_round_trips_as_json() {
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        let json = serde_json::to_string(&plan.launch_spec).expect("serialize launch spec");
        let decoded: AppleVzLaunchSpec =
            serde_json::from_str(&json).expect("deserialize launch spec");

        assert_eq!(decoded, plan.launch_spec);
    }

    #[test]
    fn writes_launch_spec_artifact_to_metadata_directory() {
        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-launch-spec-artifact-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, &temp).unwrap();

        let path = write_launch_spec_artifact(&temp, plan.launch_spec()).unwrap();
        let decoded = read_launch_spec_artifact(&path).unwrap();

        assert_eq!(path, launch_spec_path(&temp));
        assert_eq!(decoded, *plan.launch_spec());

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn builds_launch_handoff_from_artifact_spec() {
        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-launch-handoff-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, &temp).unwrap();
        let path = write_launch_spec_artifact(&temp, plan.launch_spec()).unwrap();
        let decoded = read_launch_spec_artifact(&path).unwrap();

        let handoff = build_launch_handoff(&decoded, Some(&path));

        assert_eq!(handoff.backend, "apple-virtualization-framework");
        assert_eq!(handoff.vm_name, "Ubuntu Fast");
        assert_eq!(
            handoff.launch_spec_path.as_deref(),
            Some(path.to_str().unwrap())
        );
        assert_eq!(handoff.guest, decoded.guest);
        assert_eq!(handoff.boot_mode, decoded.boot.mode);
        assert_eq!(handoff.disk, decoded.disk);
        assert_eq!(handoff.resources, decoded.resources);
        assert_eq!(handoff.runner_log_path, decoded.logs.runner_log_path);
        assert_eq!(handoff.serial_log_path, decoded.devices.serial_log_path);
        assert_eq!(handoff.integration, decoded.integration);
        assert_eq!(handoff.readiness, decoded.readiness);

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn unsupported_launcher_consumes_handoff_without_claiming_launch() {
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let mut handoff = build_launch_handoff(plan.launch_spec(), None);
        handoff.readiness = AppleVzReadinessSpec {
            ready: true,
            blockers: Vec::new(),
        };

        let error = launch_with_apple_vz(&UnsupportedAppleVzLauncher, handoff.clone())
            .expect_err("default Apple VZ launcher must stay unimplemented");

        match error {
            AppleVzLaunchError::Unsupported {
                message,
                handoff: returned_handoff,
            } => {
                assert!(message.contains("not implemented yet"));
                assert_eq!(*returned_handoff, handoff);
            }
            other => panic!("expected unsupported launch error, got {other:?}"),
        }
    }

    #[cfg(unix)]
    #[test]
    fn command_launcher_sends_handoff_to_helper_stdin() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-command-launcher-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let helper = temp.join("helper.sh");
        let captured = temp.join("handoff.json");
        std::fs::write(
            &helper,
            format!("#!/bin/sh\ncat > '{}'\n", captured.display()),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let mut handoff = build_launch_handoff(plan.launch_spec(), None);
        handoff.readiness = AppleVzReadinessSpec {
            ready: true,
            blockers: Vec::new(),
        };

        let attempt = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff).unwrap();

        assert_eq!(attempt.backend, "apple-virtualization-framework");
        assert_eq!(attempt.vm_name, "Ubuntu Fast");
        assert_eq!(attempt.stdout, "");
        assert_eq!(attempt.stderr, "");
        let captured_json = std::fs::read_to_string(&captured).unwrap();
        assert!(captured_json.contains("\"vm_name\":\"Ubuntu Fast\""));

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn command_launcher_sends_configured_helper_arguments_and_environment() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-command-launcher-args-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let helper = temp.join("helper.sh");
        let captured_args = temp.join("args.txt");
        let captured_env = temp.join("env.txt");
        std::fs::write(
            &helper,
            format!(
                "#!/bin/sh\ncat >/dev/null\nprintf '%s\\n' \"$@\" > '{}'\nprintf '%s\\n' \"$BRIDGEVM_APPLE_VZ_ALLOW_REAL_START\" > '{}'\n",
                captured_args.display(),
                captured_env.display()
            ),
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let mut handoff = build_launch_handoff(plan.launch_spec(), None);
        handoff.readiness = AppleVzReadinessSpec {
            ready: true,
            blockers: Vec::new(),
        };

        launch_with_apple_vz(
            &AppleVzCommandLauncher::new(&helper)
                .arg("--allow-real-vz-start")
                .env("BRIDGEVM_APPLE_VZ_ALLOW_REAL_START", "1"),
            handoff,
        )
        .unwrap();

        let captured_args = std::fs::read_to_string(&captured_args).unwrap();
        assert_eq!(captured_args.trim(), "--allow-real-vz-start");
        let captured_env = std::fs::read_to_string(&captured_env).unwrap();
        assert_eq!(captured_env.trim(), "1");

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn command_launcher_preserves_successful_helper_output() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-command-launcher-output-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let helper = temp.join("helper.sh");
        std::fs::write(
            &helper,
            "#!/bin/sh\ncat >/dev/null\necho 'AppleVzRunner starting VM: Ubuntu Fast'\necho 'AppleVzRunner VM finished: Ubuntu Fast (stopped)'\necho 'runner diagnostic on stderr' >&2\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let mut handoff = build_launch_handoff(plan.launch_spec(), None);
        handoff.readiness = AppleVzReadinessSpec {
            ready: true,
            blockers: Vec::new(),
        };

        let attempt = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff).unwrap();

        assert!(attempt
            .stdout
            .contains("AppleVzRunner starting VM: Ubuntu Fast"));
        assert!(attempt
            .stdout
            .contains("AppleVzRunner VM finished: Ubuntu Fast (stopped)"));
        assert_eq!(attempt.stderr, "runner diagnostic on stderr");

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[cfg(unix)]
    #[test]
    fn command_launcher_reports_helper_failure() {
        use std::os::unix::fs::PermissionsExt;

        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-command-launcher-fail-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(&temp).unwrap();
        let helper = temp.join("helper.sh");
        std::fs::write(
            &helper,
            "#!/bin/sh\ncat >/dev/null\necho 'ready summary on stdout'\necho 'not implemented yet' >&2\nexit 2\n",
        )
        .unwrap();
        let mut permissions = std::fs::metadata(&helper).unwrap().permissions();
        permissions.set_mode(0o755);
        std::fs::set_permissions(&helper, permissions).unwrap();

        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let mut handoff = build_launch_handoff(plan.launch_spec(), None);
        handoff.readiness = AppleVzReadinessSpec {
            ready: true,
            blockers: Vec::new(),
        };

        let error = launch_with_apple_vz(&AppleVzCommandLauncher::new(&helper), handoff)
            .expect_err("helper failure must surface");

        match error {
            AppleVzLaunchError::LauncherFailed { stdout, stderr, .. } => {
                assert_eq!(stderr, "not implemented yet");
                assert_eq!(stdout, "ready summary on stdout");
            }
            other => panic!("expected helper failure, got {other:?}"),
        }

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn launch_handoff_readiness_blocks_before_launcher_runs() {
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let handoff = build_launch_handoff(plan.launch_spec(), None);

        let error = launch_with_apple_vz(&UnsupportedAppleVzLauncher, handoff.clone())
            .expect_err("not-ready handoff must be rejected before launch");

        match error {
            AppleVzLaunchError::NotReady { blockers } => {
                assert_eq!(blockers, handoff.readiness.blockers);
            }
            other => panic!("expected readiness launch error, got {other:?}"),
        }
    }

    #[test]
    fn rejects_compatibility_manifest() {
        let manifest = VmManifest::new(
            "Legacy",
            VmMode::Compatibility,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "x86_64".to_string(),
            },
            "64GiB",
        );
        assert!(build_fast_plan(&manifest, Path::new("/tmp/legacy.vmbridge")).is_err());
    }

    #[test]
    fn accepts_aarch64_guest_arch() {
        let mut manifest = valid_fast_manifest();
        manifest.guest.arch = "aarch64".to_string();

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        assert_eq!(plan.guest_arch, "aarch64");
    }

    #[test]
    fn rejects_unsupported_guest_arch() {
        let mut manifest = valid_fast_manifest();
        manifest.guest.arch = "x86_64".to_string();

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("x86_64 guests must not enter Apple VZ launch planning");

        assert!(matches!(error, AppleVzError::UnsupportedGuestArch(arch) if arch == "x86_64"));
    }

    #[test]
    fn rejects_unsupported_preferred_backend() {
        let mut manifest = valid_fast_manifest();
        manifest.backend.preferred = Some("qemu".to_string());

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("non-Apple VZ preferred backend must be rejected");

        assert!(
            matches!(error, AppleVzError::UnsupportedPreferredBackend(backend) if backend == "qemu")
        );
    }

    #[test]
    fn accepts_unset_preferred_backend() {
        let mut manifest = valid_fast_manifest();
        manifest.backend.preferred = None;

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        assert_eq!(plan.vm_name, "Ubuntu Fast");
    }

    #[test]
    fn plans_linux_installer_media() {
        let mut manifest = valid_fast_manifest();
        manifest.boot = Some(Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let installer = plan.launch_spec.boot.installer_image.as_ref().unwrap();

        assert_eq!(plan.launch_spec.boot.mode, BootMode::LinuxInstaller);
        assert!(installer
            .path
            .ends_with("/tmp/ubuntu-fast.vmbridge/installers/ubuntu-arm64.iso"));
        assert!(!installer.exists);
        assert!(plan
            .launch_spec
            .readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-installer-image"));
    }

    #[test]
    fn plans_linux_kernel_boot_inputs() {
        let mut manifest = valid_fast_manifest();
        manifest.boot = Some(Boot {
            mode: BootMode::LinuxKernel,
            installer_image: None,
            kernel_path: Some("boot/vmlinuz".to_string()),
            initrd_path: Some("boot/initrd".to_string()),
            kernel_command_line: Some("console=hvc0".to_string()),
            macos_restore_image: None,
        });

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let kernel = plan.launch_spec.boot.kernel.as_ref().unwrap();
        let initrd = plan.launch_spec.boot.initrd.as_ref().unwrap();

        assert_eq!(plan.launch_spec.boot.mode, BootMode::LinuxKernel);
        assert_eq!(plan.config.kernel_path, Some(kernel.path.clone()));
        assert!(kernel
            .path
            .ends_with("/tmp/ubuntu-fast.vmbridge/boot/vmlinuz"));
        assert!(initrd
            .path
            .ends_with("/tmp/ubuntu-fast.vmbridge/boot/initrd"));
        assert_eq!(
            plan.launch_spec.boot.kernel_command_line.as_deref(),
            Some("console=hvc0")
        );
        assert!(plan
            .launch_spec
            .readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-kernel"));
        assert!(plan
            .launch_spec
            .readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-initrd"));
    }

    #[test]
    fn plans_macos_restore_image() {
        let mut manifest = valid_fast_manifest();
        manifest.guest.os = "macos".to_string();
        manifest.boot = Some(Boot {
            mode: BootMode::MacosRestore,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: Some("/Library/BridgeVM/restore.ipsw".to_string()),
        });

        let plan = build_fast_plan(&manifest, Path::new("/tmp/macos.vmbridge")).unwrap();
        let restore = plan.launch_spec.boot.macos_restore_image.as_ref().unwrap();

        assert_eq!(plan.launch_spec.boot.mode, BootMode::MacosRestore);
        assert_eq!(restore.path, "/Library/BridgeVM/restore.ipsw");
        assert!(plan
            .launch_spec
            .readiness
            .blockers
            .iter()
            .any(|blocker| blocker.code == "missing-macos-restore-image"));
    }

    #[test]
    fn launch_readiness_is_ready_when_required_paths_exist() {
        let temp = std::env::temp_dir().join(format!(
            "bridgevm-apple-vz-readiness-{}",
            std::process::id()
        ));
        let _ = std::fs::remove_dir_all(&temp);
        std::fs::create_dir_all(temp.join("disks")).unwrap();
        std::fs::create_dir_all(temp.join("installers")).unwrap();
        std::fs::write(temp.join("disks/root.qcow2"), b"disk").unwrap();
        std::fs::write(temp.join("installers/ubuntu-arm64.iso"), b"iso").unwrap();

        let mut manifest = valid_fast_manifest();
        manifest.boot = Some(Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: Some("installers/ubuntu-arm64.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let plan = build_fast_plan(&manifest, &temp).unwrap();

        if AppleVzHostCapability::current().is_macos()
            && AppleVzHostCapability::current().is_apple_silicon()
        {
            assert!(plan.launch_spec.readiness.ready);
            assert!(plan.launch_spec.readiness.blockers.is_empty());
        } else {
            assert!(!plan.launch_spec.readiness.ready);
            assert!(plan
                .launch_spec
                .readiness
                .blockers
                .iter()
                .all(|blocker| blocker.path.is_none()));
        }

        let _ = std::fs::remove_dir_all(&temp);
    }

    #[test]
    fn launch_readiness_reports_unsupported_host_capabilities() {
        let boot = AppleVzBootSpec {
            mode: BootMode::ExistingDisk,
            installer_image: None,
            kernel: None,
            initrd: None,
            kernel_command_line: None,
            macos_restore_image: None,
        };
        let host = AppleVzHostCapability {
            os: "linux".to_string(),
            arch: "x86_64".to_string(),
        };
        let readiness = build_readiness_spec(&boot, "/tmp/root.qcow2", &host);

        assert!(!readiness.ready);
        assert!(readiness.blockers.iter().any(|blocker| {
            blocker.code == "unsupported-host-os"
                && blocker.capability.as_deref() == Some("apple-virtualization-framework")
        }));
        assert!(readiness.blockers.iter().any(|blocker| {
            blocker.code == "unsupported-host-arch"
                && blocker.capability.as_deref() == Some("apple-silicon")
        }));
    }

    #[test]
    fn rejects_windows_guest_for_apple_vz() {
        let mut manifest = valid_fast_manifest();
        manifest.guest.os = "windows".to_string();

        let error = build_fast_plan(&manifest, Path::new("/tmp/windows.vmbridge"))
            .expect_err("Windows Fast Mode should use the restricted backend, not Apple VZ");

        assert!(matches!(error, AppleVzError::UnsupportedGuestOs(os) if os == "windows"));
    }

    #[test]
    fn rejects_missing_linux_installer_image() {
        let mut manifest = valid_fast_manifest();
        manifest.boot = Some(Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("linux-installer boot requires installerImage");

        assert!(matches!(
            error,
            AppleVzError::MissingBootInput {
                mode: BootMode::LinuxInstaller,
                field: "installerImage"
            }
        ));
    }

    #[test]
    fn rejects_macos_restore_for_linux_guest() {
        let mut manifest = valid_fast_manifest();
        manifest.boot = Some(Boot {
            mode: BootMode::MacosRestore,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: Some("restore.ipsw".to_string()),
        });

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("macOS restore mode must not be accepted for Linux guests");

        assert!(
            matches!(error, AppleVzError::InvalidBootModeForGuest { guest_os, mode: BootMode::MacosRestore } if guest_os == "ubuntu")
        );
    }

    #[test]
    fn rejects_unsupported_network_mode() {
        let mut manifest = valid_fast_manifest();
        manifest.network.mode = "bridged".to_string();

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("non-nat networking must be rejected");

        assert!(matches!(error, AppleVzError::UnsupportedNetworkMode(mode) if mode == "bridged"));
    }

    #[test]
    fn rejects_planned_host_only_network_mode_until_apple_vz_support_exists() {
        let mut manifest = valid_fast_manifest();
        manifest.network.mode = "host-only".to_string();

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("Apple VZ launch boundary still accepts only NAT");

        assert!(matches!(error, AppleVzError::UnsupportedNetworkMode(mode) if mode == "host-only"));
    }

    #[test]
    fn rejects_unknown_network_mode_name_through_planner_parsing() {
        let mut manifest = valid_fast_manifest();
        manifest.network.mode = "slirp".to_string();

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("unknown network modes must be rejected");

        assert!(matches!(error, AppleVzError::UnsupportedNetworkMode(mode) if mode == "slirp"));
    }

    #[test]
    fn accepts_raw_primary_disk() {
        let mut manifest = valid_fast_manifest();
        manifest.storage.primary.format = "raw".to_string();
        manifest.storage.primary.path = "disks/root.raw".to_string();

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        assert!(plan.config.disk_path.ends_with("disks/root.raw"));
    }

    #[test]
    fn rejects_unsupported_primary_disk_format() {
        let mut manifest = valid_fast_manifest();
        manifest.storage.primary.format = "vmdk".to_string();

        let error = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge"))
            .expect_err("non-raw/qcow2 primary disks must be rejected");

        assert!(
            matches!(error, AppleVzError::UnsupportedPrimaryDiskFormat(format) if format == "vmdk")
        );
    }

    #[test]
    fn build_fast_plan_populates_share_from_first_approved_folder() {
        let mut manifest = valid_fast_manifest();
        manifest.integration.shared_folders = true;
        manifest.shared_folders = vec![shared_folder("workspace", "/Users/me/work", true)];

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        assert_eq!(plan.launch_spec.shares.len(), 1);
        let share = &plan.launch_spec.shares[0];

        assert_eq!(share.host_path, "/Users/me/work");
        assert_eq!(share.tag, "workspace");
        assert!(share.read_only);
    }

    #[test]
    fn build_fast_plan_populates_all_approved_folders() {
        let mut manifest = valid_fast_manifest();
        manifest.integration.shared_folders = true;
        manifest.shared_folders = vec![
            shared_folder("first", "/Users/me/first", false),
            shared_folder("second", "/Users/me/second", true),
            shared_folder("third", "/Users/me/third", false),
        ];

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        assert_eq!(plan.launch_spec.shares.len(), 3);
        assert_eq!(plan.launch_spec.shares[0].tag, "first");
        assert_eq!(plan.launch_spec.shares[0].host_path, "/Users/me/first");
        assert!(!plan.launch_spec.shares[0].read_only);
        assert_eq!(plan.launch_spec.shares[1].tag, "second");
        assert_eq!(plan.launch_spec.shares[1].host_path, "/Users/me/second");
        assert!(plan.launch_spec.shares[1].read_only);
        assert_eq!(plan.launch_spec.shares[2].tag, "third");
        assert_eq!(plan.launch_spec.shares[2].host_path, "/Users/me/third");
    }

    #[test]
    fn build_fast_plan_omits_shares_when_no_folders_present() {
        let mut manifest = valid_fast_manifest();
        manifest.integration.shared_folders = true;
        manifest.shared_folders = Vec::new();

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        assert!(plan.launch_spec.shares.is_empty());
    }

    #[test]
    fn build_fast_plan_omits_shares_when_integration_flag_off() {
        let mut manifest = valid_fast_manifest();
        manifest.integration.shared_folders = false;
        manifest.shared_folders = vec![shared_folder("workspace", "/Users/me/work", false)];

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();

        assert!(plan.launch_spec.shares.is_empty());
    }

    #[test]
    fn build_share_specs_defaults_tag_to_share_for_empty_name() {
        let shares = build_share_specs(true, &[shared_folder("", "/Users/me/work", false)]);
        assert_eq!(shares.len(), 1);

        assert_eq!(shares[0].tag, "share");
        assert_eq!(shares[0].host_path, "/Users/me/work");
        assert!(!shares[0].read_only);
    }

    #[test]
    fn build_share_specs_disambiguates_duplicate_and_empty_tags() {
        // Two unnamed folders both default to "share"; a third explicitly named
        // "share" also collides. Tags must stay unique for VZ.
        let folders = vec![
            shared_folder("", "/Users/me/a", false),
            shared_folder("", "/Users/me/b", true),
            shared_folder("share", "/Users/me/c", false),
            shared_folder("docs", "/Users/me/d", false),
            shared_folder("docs", "/Users/me/e", false),
        ];
        let shares = build_share_specs(true, &folders);

        let tags: Vec<&str> = shares.iter().map(|s| s.tag.as_str()).collect();
        assert_eq!(tags, ["share", "share-2", "share-3", "docs", "docs-2"]);
        // Host paths and read-only flags stay paired to their folders.
        assert_eq!(shares[0].host_path, "/Users/me/a");
        assert_eq!(shares[1].host_path, "/Users/me/b");
        assert!(shares[1].read_only);
    }

    #[test]
    fn encode_share_flag_value_round_trips_tag_path_and_read_only() {
        let writable = AppleVzShareSpec {
            host_path: "/Users/me/work".to_string(),
            tag: "workspace".to_string(),
            read_only: false,
        };
        assert_eq!(encode_share_flag_value(&writable), "workspace=/Users/me/work");

        let read_only = AppleVzShareSpec {
            host_path: "/Users/me/with=equals and spaces".to_string(),
            tag: "weird".to_string(),
            read_only: true,
        };
        // Split-on-first-`=` keeps the host path (with its own `=`/spaces) intact.
        let value = encode_share_flag_value(&read_only);
        assert_eq!(value, "ro:weird=/Users/me/with=equals and spaces");
        let stripped = value.strip_prefix("ro:").unwrap();
        let (tag, path) = stripped.split_once('=').unwrap();
        assert_eq!(tag, "weird");
        assert_eq!(path, "/Users/me/with=equals and spaces");
    }

    #[test]
    fn render_runner_words_emits_one_share_flag_per_folder() {
        let mut manifest = valid_fast_manifest();
        manifest.integration.shared_folders = true;
        manifest.shared_folders = vec![
            shared_folder("workspace", "/Users/me/work", true),
            shared_folder("docs", "/Users/me/docs", false),
        ];

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let words = plan.render_runner_words();

        let share_positions: Vec<usize> = words
            .iter()
            .enumerate()
            .filter(|(_, w)| *w == "--share")
            .map(|(i, _)| i)
            .collect();
        assert_eq!(share_positions.len(), 2);
        assert_eq!(words[share_positions[0] + 1], "ro:workspace=/Users/me/work");
        assert_eq!(words[share_positions[1] + 1], "docs=/Users/me/docs");
        // Old single-share flags are no longer emitted by the planner.
        assert!(!words.iter().any(|w| w == "--share-dir"));
    }

    #[test]
    fn render_runner_words_emits_no_share_flags_when_shares_absent() {
        let manifest = valid_fast_manifest();
        // Default manifest has no approved folders, so no shares are planned.
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let words = plan.render_runner_words();

        assert!(plan.launch_spec.shares.is_empty());
        assert!(!words.iter().any(|w| w == "--share"));
        assert!(!words.iter().any(|w| w == "--share-dir"));
    }

    #[test]
    fn build_launch_handoff_carries_all_shares_through() {
        let mut manifest = valid_fast_manifest();
        manifest.integration.shared_folders = true;
        manifest.shared_folders = vec![
            shared_folder("workspace", "/Users/me/work", true),
            shared_folder("docs", "/Users/me/docs", false),
        ];

        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let handoff = build_launch_handoff(plan.launch_spec(), None);

        assert_eq!(handoff.shares.len(), 2);
        assert_eq!(handoff.shares[0].tag, "workspace");
        assert_eq!(handoff.shares[0].host_path, "/Users/me/work");
        assert!(handoff.shares[0].read_only);
        assert_eq!(handoff.shares[1].tag, "docs");
        assert_eq!(handoff.shares[1].host_path, "/Users/me/docs");
        assert!(!handoff.shares[1].read_only);
    }

    #[test]
    fn build_launch_handoff_omits_shares_when_none_planned() {
        let manifest = valid_fast_manifest();
        let plan = build_fast_plan(&manifest, Path::new("/tmp/ubuntu-fast.vmbridge")).unwrap();
        let handoff = build_launch_handoff(plan.launch_spec(), None);

        assert!(handoff.shares.is_empty());
    }
}
