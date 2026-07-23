//! The serde data model for vm-manifest-v1 and the inherent/Display impls on it.

use crate::*;
use serde::Deserialize;
use serde::Serialize;
use std::fmt;

pub const SCHEMA_VERSION: &str = "bridgevm.io/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmMode {
    Fast,
    Compatibility,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct VmManifest {
    #[serde(rename = "schemaVersion")]
    pub schema_version: String,
    pub name: String,
    pub mode: VmMode,
    pub guest: Guest,
    pub backend: Backend,
    pub resources: Resources,
    pub display: Display,
    pub storage: Storage,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub boot: Option<Boot>,
    pub network: Network,
    pub integration: Integration,
    pub security: Security,
    #[serde(rename = "sharedFolders", default)]
    pub shared_folders: Vec<SharedFolder>,
    #[serde(default, skip_serializing_if = "Firmware::is_default")]
    pub firmware: Firmware,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Guest {
    pub os: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    pub arch: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Backend {
    pub engine: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub preferred: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accelerator: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Resources {
    pub profile: String,
    pub memory: String,
    pub cpu: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Display {
    pub renderer: String,
    #[serde(rename = "framePolicy")]
    pub frame_policy: String,
    pub retina: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Storage {
    pub primary: PrimaryDisk,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrimaryDisk {
    pub path: String,
    pub size: String,
    pub format: String,
    pub discard: bool,
}

/// Optional Windows 11-class firmware/hardware requirements for Compatibility
/// Mode (QEMU aarch64). Every field defaults off, so existing manifests and the
/// already-proven installer-reachability path are unchanged. Enabling a field
/// wires the corresponding QEMU device(s) for a full Windows 11 install; each
/// also needs an external host resource at runtime (see field docs). Ignored
/// outside aarch64 Compatibility Mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Firmware {
    /// Attach the primary disk as an NVMe device (`-device nvme`) instead of
    /// virtio-blk. Windows 11 Setup recognizes NVMe natively, so no injected
    /// virtio driver is needed to see the install target.
    #[serde(rename = "nvmeTarget", default)]
    pub nvme_target: bool,
    /// Attach an emulated TPM 2.0 (`tpm-tis-device`) backed by an external
    /// `swtpm` process listening on `<bundle>/metadata/swtpm.sock`. The swtpm
    /// process must be started separately (host dependency).
    #[serde(default)]
    pub tpm: bool,
    /// Boot a Secure Boot-capable UEFI: a read-only edk2 code pflash plus a
    /// writable per-bundle variable store (`<bundle>/metadata/edk2-vars.fd`)
    /// instead of the plain read-only `-bios`. The varstore must be seeded from
    /// an edk2 secure-boot template with Microsoft keys enrolled (host
    /// resource); persisting it in the bundle is what lets Secure Boot state
    /// survive across boots.
    #[serde(rename = "secureBoot", default)]
    pub secure_boot: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Boot {
    pub mode: BootMode,
    #[serde(
        rename = "installerImage",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub installer_image: Option<String>,
    #[serde(
        rename = "kernelPath",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_path: Option<String>,
    #[serde(
        rename = "initrdPath",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub initrd_path: Option<String>,
    #[serde(
        rename = "kernelCommandLine",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub kernel_command_line: Option<String>,
    #[serde(
        rename = "macosRestoreImage",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub macos_restore_image: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BootMode {
    ExistingDisk,
    LinuxKernel,
    LinuxInstaller,
    WindowsInstaller,
    MacosRestore,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Network {
    pub mode: String,
    pub hostname: String,
    #[serde(default)]
    pub forwards: Vec<PortForward>,
    /// Host interface that bridged (`mode: bridged`) networking attaches the
    /// guest to. Only consulted in Compatibility Mode QEMU bridged networking;
    /// defaults to [`DEFAULT_BRIDGE_INTERFACE`] when omitted so existing
    /// manifests (which never carried this field) keep deserializing.
    #[serde(
        rename = "bridgeInterface",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub bridge_interface: Option<String>,
}

/// Default host interface bridged networking attaches to when a manifest does
/// not pin one with `network.bridgeInterface`. `en0` is the primary
/// Ethernet/Wi-Fi interface on a typical Mac.
pub const DEFAULT_BRIDGE_INTERFACE: &str = "en0";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PortForward {
    pub host: u16,
    pub guest: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SharedFolder {
    pub name: String,
    #[serde(rename = "hostPath")]
    pub host_path: String,
    #[serde(rename = "readOnly", default)]
    pub read_only: bool,
    #[serde(
        rename = "hostPathToken",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub host_path_token: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Integration {
    pub tools: String,
    pub clipboard: bool,
    #[serde(rename = "dragDrop")]
    pub drag_drop: bool,
    #[serde(rename = "dynamicResolution")]
    pub dynamic_resolution: bool,
    #[serde(rename = "sharedFolders")]
    pub shared_folders: bool,
    #[serde(default)]
    pub applications: bool,
    #[serde(default)]
    pub windows: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Security {
    #[serde(rename = "sharedFolderApproval")]
    pub shared_folder_approval: String,
    #[serde(rename = "guestCommandExecution")]
    pub guest_command_execution: bool,
    #[serde(rename = "signedAgentUpdates")]
    pub signed_agent_updates: bool,
}

impl fmt::Display for VmMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmMode::Fast => write!(f, "fast"),
            VmMode::Compatibility => write!(f, "compatibility"),
        }
    }
}

impl Firmware {
    /// True when no firmware feature is requested — used to keep legacy
    /// manifests byte-stable on round-trip (the section is omitted entirely).
    pub fn is_default(&self) -> bool {
        *self == Firmware::default()
    }
}

impl fmt::Display for BootMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BootMode::ExistingDisk => write!(f, "existing-disk"),
            BootMode::LinuxKernel => write!(f, "linux-kernel"),
            BootMode::LinuxInstaller => write!(f, "linux-installer"),
            BootMode::WindowsInstaller => write!(f, "windows-installer"),
            BootMode::MacosRestore => write!(f, "macos-restore"),
        }
    }
}

impl Network {
    /// The host interface bridged networking should attach to: the manifest's
    /// `bridgeInterface` if set (and non-blank), otherwise
    /// [`DEFAULT_BRIDGE_INTERFACE`].
    pub fn bridge_interface(&self) -> &str {
        self.bridge_interface
            .as_deref()
            .map(str::trim)
            .filter(|iface| !iface.is_empty())
            .unwrap_or(DEFAULT_BRIDGE_INTERFACE)
    }
}

impl SharedFolder {
    pub fn resolved_host_path_token(&self) -> String {
        self.host_path_token
            .clone()
            .unwrap_or_else(|| stable_share_token(&self.name, &self.host_path))
    }
}
