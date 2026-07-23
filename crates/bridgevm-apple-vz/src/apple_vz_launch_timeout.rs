//! Split out of lib.rs to keep files under 800 lines.

use crate::*;
use bridgevm_config::BootMode;
use bridgevm_config::SharedFolder;
use bridgevm_config::VmManifest;
use bridgevm_config::VmMode;
use bridgevm_network::NetworkPlanError;
use bridgevm_resource_manager::decide_from_manifest_profile;
use bridgevm_resource_manager::resolve_memory;
use bridgevm_resource_manager::resolve_vcpu;
use serde::Deserialize;
use serde::Serialize;
use std::fs;
use std::io::Read;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;
use std::process::Stdio;
use std::thread;
use std::time::Duration;
use std::time::Instant;
use thiserror::Error;

pub(crate) const APPLE_VZ_LAUNCH_TIMEOUT: Duration = Duration::from_secs(30);
pub(crate) const MAX_LAUNCHER_STREAM_BYTES: usize = 1024 * 1024;
pub(crate) const LAUNCHER_DRAIN_CHUNK_BYTES: usize = 64 * 1024;

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
pub(crate) const DEFAULT_SHARE_TAG: &str = "share";

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
pub(crate) fn build_share_specs(
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
pub(crate) fn unique_tag(base: &str, used: &[String]) -> String {
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
            message:
                "Apple Virtualization.framework launch requires --apple-vz-runner to point at a signed AppleVzRunner"
                    .to_string(),
            handoff: Box::new(handoff),
        })
    }
}

#[derive(Debug, Clone)]
pub struct AppleVzCommandLauncher {
    pub(crate) program: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) env: Vec<(String, String)>,
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
        let mut stdout = child.stdout.take().expect("piped stdout should be present");
        let mut stderr = child.stderr.take().expect("piped stderr should be present");
        let stdout_drain =
            thread::spawn(move || drain_launcher_stream(&mut stdout, MAX_LAUNCHER_STREAM_BYTES));
        let stderr_drain =
            thread::spawn(move || drain_launcher_stream(&mut stderr, MAX_LAUNCHER_STREAM_BYTES));
        let mut stdin = child.stdin.take().expect("piped stdin should be present");
        if let Err(source) = stdin.write_all(&input) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout_drain.join();
            let _ = stderr_drain.join();
            return Err(AppleVzLaunchError::LauncherWrite {
                program: self.program.clone(),
                source,
            });
        }
        drop(stdin);
        let deadline = Instant::now() + APPLE_VZ_LAUNCH_TIMEOUT;
        let status = loop {
            match child.try_wait() {
                Ok(Some(status)) => break status,
                Ok(None) if Instant::now() < deadline => thread::sleep(Duration::from_millis(20)),
                Ok(None) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_drain.join();
                    let _ = stderr_drain.join();
                    return Err(AppleVzLaunchError::LauncherTimeout {
                        program: self.program.clone(),
                        seconds: APPLE_VZ_LAUNCH_TIMEOUT.as_secs(),
                    });
                }
                Err(source) => {
                    let _ = child.kill();
                    let _ = child.wait();
                    let _ = stdout_drain.join();
                    let _ = stderr_drain.join();
                    return Err(AppleVzLaunchError::LauncherSpawn {
                        program: self.program.clone(),
                        source,
                    });
                }
            }
        };
        let (stdout_bytes, stdout_exceeded) = join_launcher_drain(stdout_drain, &self.program)?;
        let (stderr_bytes, stderr_exceeded) = join_launcher_drain(stderr_drain, &self.program)?;
        if stdout_exceeded || stderr_exceeded {
            return Err(AppleVzLaunchError::LauncherOutputTooLarge {
                program: self.program.clone(),
                stream: if stdout_exceeded { "stdout" } else { "stderr" },
                maximum: MAX_LAUNCHER_STREAM_BYTES,
            });
        }
        let stdout = String::from_utf8_lossy(&stdout_bytes).trim().to_string();
        let stderr = String::from_utf8_lossy(&stderr_bytes).trim().to_string();
        if !status.success() {
            return Err(AppleVzLaunchError::LauncherFailed {
                program: self.program.clone(),
                status: status.to_string(),
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

pub(crate) fn drain_launcher_stream(
    reader: &mut impl Read,
    maximum: usize,
) -> std::io::Result<(Vec<u8>, bool)> {
    let mut captured = Vec::new();
    let mut chunk = [0_u8; LAUNCHER_DRAIN_CHUNK_BYTES];
    let mut exceeded = false;
    loop {
        let read = reader.read(&mut chunk)?;
        if read == 0 {
            break;
        }
        let keep = read.min(maximum.saturating_sub(captured.len()));
        captured.extend_from_slice(&chunk[..keep]);
        exceeded |= keep < read;
    }
    Ok((captured, exceeded))
}

pub(crate) fn join_launcher_drain(
    drain: thread::JoinHandle<std::io::Result<(Vec<u8>, bool)>>,
    program: &Path,
) -> Result<(Vec<u8>, bool), AppleVzLaunchError> {
    drain
        .join()
        .map_err(|_| AppleVzLaunchError::LauncherSpawn {
            program: program.to_path_buf(),
            source: std::io::Error::other("launcher output drain panicked"),
        })?
        .map_err(|source| AppleVzLaunchError::LauncherSpawn {
            program: program.to_path_buf(),
            source,
        })
}

pub(crate) fn format_launcher_output(stdout: &str, stderr: &str) -> String {
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
        vec!["lightvm-runner".to_string(), self.vm_name.clone()]
    }

    pub fn render_runner_words_for_launch_spec(&self, launch_spec_path: &Path) -> Vec<String> {
        vec![
            "lightvm-runner".to_string(),
            "--launch-spec".to_string(),
            launch_spec_path.display().to_string(),
        ]
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
    let readiness = build_readiness_spec(
        &boot,
        &disk_path,
        &manifest.storage.primary.format,
        &AppleVzHostCapability::current(),
    );
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
    const MAX_LAUNCH_SPEC_BYTES: u64 = 1024 * 1024;
    let mut bytes = Vec::new();
    fs::File::open(path)
        .and_then(|file| file.take(MAX_LAUNCH_SPEC_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|source| AppleVzLaunchSpecArtifactError::Read {
            path: path.to_path_buf(),
            source,
        })?;
    if bytes.len() as u64 > MAX_LAUNCH_SPEC_BYTES {
        return Err(AppleVzLaunchSpecArtifactError::TooLarge {
            path: path.to_path_buf(),
            maximum: MAX_LAUNCH_SPEC_BYTES,
        });
    }
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

pub(crate) fn preflight_apple_vz_launch(manifest: &VmManifest) -> Result<(), AppleVzError> {
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
