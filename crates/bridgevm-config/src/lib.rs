use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::{fmt, fs, path::Path};
use thiserror::Error;

pub const SCHEMA_VERSION: &str = "bridgevm.io/v1";
pub const MANIFEST_JSON_SCHEMA_ID: &str = "https://bridgevm.io/schemas/vm-manifest-v1.schema.json";

pub fn manifest_json_schema_v1() -> &'static str {
    r#"{
  "$schema": "https://json-schema.org/draft/2020-12/schema",
  "$id": "https://bridgevm.io/schemas/vm-manifest-v1.schema.json",
  "title": "BridgeVM VM Manifest",
  "type": "object",
  "additionalProperties": false,
  "required": [
    "schemaVersion",
    "name",
    "mode",
    "guest",
    "backend",
    "resources",
    "display",
    "storage",
    "network",
    "integration",
    "security"
  ],
  "properties": {
    "schemaVersion": {
      "const": "bridgevm.io/v1"
    },
    "name": {
      "type": "string",
      "minLength": 1
    },
    "mode": {
      "enum": ["fast", "compatibility"]
    },
    "guest": {
      "type": "object",
      "additionalProperties": false,
      "required": ["os", "arch"],
      "properties": {
        "os": { "type": "string", "minLength": 1 },
        "version": { "type": "string" },
        "arch": { "type": "string", "minLength": 1 }
      }
    },
    "backend": {
      "type": "object",
      "additionalProperties": false,
      "required": ["engine"],
      "properties": {
        "engine": { "type": "string", "minLength": 1 },
        "preferred": { "type": "string" },
        "fallback": { "type": "string" },
        "accelerator": { "type": "string" }
      }
    },
    "resources": {
      "type": "object",
      "additionalProperties": false,
      "required": ["profile", "memory", "cpu"],
      "properties": {
        "profile": { "type": "string", "minLength": 1 },
        "memory": { "type": "string", "minLength": 1 },
        "cpu": { "type": "string", "minLength": 1 }
      }
    },
    "display": {
      "type": "object",
      "additionalProperties": false,
      "required": ["renderer", "framePolicy", "retina"],
      "properties": {
        "renderer": { "type": "string", "minLength": 1 },
        "framePolicy": { "type": "string", "minLength": 1 },
        "retina": { "type": "boolean" }
      }
    },
    "storage": {
      "type": "object",
      "additionalProperties": false,
      "required": ["primary"],
      "properties": {
        "primary": {
          "type": "object",
          "additionalProperties": false,
          "required": ["path", "size", "format", "discard"],
          "properties": {
            "path": { "type": "string", "minLength": 1 },
            "size": { "type": "string", "minLength": 1 },
            "format": { "type": "string", "minLength": 1 },
            "discard": { "type": "boolean" }
          }
        }
      }
    },
    "boot": {
      "type": "object",
      "additionalProperties": false,
      "required": ["mode"],
      "properties": {
        "mode": {
          "enum": ["existing-disk", "linux-kernel", "linux-installer", "windows-installer", "macos-restore"]
        },
        "installerImage": { "type": "string" },
        "kernelPath": { "type": "string" },
        "initrdPath": { "type": "string" },
        "kernelCommandLine": { "type": "string" },
        "macosRestoreImage": { "type": "string" }
      }
    },
    "network": {
      "type": "object",
      "additionalProperties": false,
      "required": ["mode", "hostname"],
      "properties": {
        "mode": { "type": "string", "minLength": 1 },
        "hostname": { "type": "string", "minLength": 1 },
        "forwards": {
          "type": "array",
          "items": {
            "type": "object",
            "additionalProperties": false,
            "required": ["host", "guest"],
            "properties": {
              "host": { "type": "integer", "minimum": 1, "maximum": 65535 },
              "guest": { "type": "integer", "minimum": 1, "maximum": 65535 }
            }
          }
        }
      }
    },
    "integration": {
      "type": "object",
      "additionalProperties": false,
      "required": ["tools", "clipboard", "dragDrop", "dynamicResolution", "sharedFolders"],
      "properties": {
        "tools": { "type": "string", "minLength": 1 },
        "clipboard": { "type": "boolean" },
        "dragDrop": { "type": "boolean" },
        "dynamicResolution": { "type": "boolean" },
        "sharedFolders": { "type": "boolean" },
        "applications": { "type": "boolean" },
        "windows": { "type": "boolean" }
      }
    },
    "security": {
      "type": "object",
      "additionalProperties": false,
      "required": ["sharedFolderApproval", "guestCommandExecution", "signedAgentUpdates"],
      "properties": {
        "sharedFolderApproval": { "type": "string", "minLength": 1 },
        "guestCommandExecution": { "type": "boolean" },
        "signedAgentUpdates": { "type": "boolean" }
      }
    },
    "sharedFolders": {
      "type": "array",
      "items": {
        "type": "object",
        "additionalProperties": false,
        "required": ["name", "hostPath"],
        "properties": {
          "name": { "type": "string", "minLength": 1 },
          "hostPath": { "type": "string", "minLength": 1 },
          "readOnly": { "type": "boolean" },
          "hostPathToken": { "type": "string", "minLength": 1 }
        }
      }
    }
  }
}
"#
}

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("manifest schema version must be {expected}, got {actual}")]
    UnsupportedSchema {
        expected: &'static str,
        actual: String,
    },
    #[error("manifest name cannot be empty")]
    EmptyName,
    #[error("boot mode {mode} requires {field}")]
    MissingBootInput { mode: BootMode, field: &'static str },
    #[error("boot input {field} cannot be empty")]
    EmptyBootInput { field: &'static str },
    #[error("boot mode {mode} cannot use {field}")]
    UnsupportedBootInput { mode: BootMode, field: &'static str },
    #[error("shared folder {index} field {field} cannot be empty")]
    EmptySharedFolderField { index: usize, field: &'static str },
    #[error("duplicate shared folder name '{name}'")]
    DuplicateSharedFolderName { name: String },
    #[error("duplicate shared folder token '{token}'")]
    DuplicateSharedFolderToken { token: String },
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("YAML error: {0}")]
    Yaml(#[from] serde_yaml::Error),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VmMode {
    Fast,
    Compatibility,
}

impl fmt::Display for VmMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            VmMode::Fast => write!(f, "fast"),
            VmMode::Compatibility => write!(f, "compatibility"),
        }
    }
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Network {
    pub mode: String,
    pub hostname: String,
    #[serde(default)]
    pub forwards: Vec<PortForward>,
}

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

impl SharedFolder {
    pub fn resolved_host_path_token(&self) -> String {
        self.host_path_token
            .clone()
            .unwrap_or_else(|| stable_share_token(&self.name, &self.host_path))
    }
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

impl VmManifest {
    pub fn new(
        name: impl Into<String>,
        mode: VmMode,
        guest: Guest,
        disk_size: impl Into<String>,
    ) -> Self {
        let name = name.into();
        let disk_size = disk_size.into();
        let fast = mode == VmMode::Fast;
        let hostname = format!("{}.bridgevm.local", slug(&name));

        Self {
            schema_version: SCHEMA_VERSION.to_string(),
            name,
            mode,
            guest,
            backend: if fast {
                Backend {
                    engine: "lightvm".to_string(),
                    preferred: Some("apple-vz".to_string()),
                    fallback: Some("qemu-hvf-restricted".to_string()),
                    accelerator: Some("hvf".to_string()),
                }
            } else {
                Backend {
                    engine: "qemu".to_string(),
                    preferred: None,
                    fallback: Some("tcg".to_string()),
                    accelerator: Some("hvf".to_string()),
                }
            },
            resources: Resources {
                profile: "automatic".to_string(),
                memory: "auto".to_string(),
                cpu: "auto".to_string(),
            },
            display: Display {
                renderer: if fast { "metal" } else { "spice-or-vnc" }.to_string(),
                frame_policy: "adaptive".to_string(),
                retina: true,
            },
            storage: Storage {
                primary: PrimaryDisk {
                    path: "disks/root.qcow2".to_string(),
                    size: disk_size,
                    format: "qcow2".to_string(),
                    discard: fast,
                },
            },
            boot: Some(Boot {
                mode: BootMode::ExistingDisk,
                installer_image: None,
                kernel_path: None,
                initrd_path: None,
                kernel_command_line: None,
                macos_restore_image: None,
            }),
            network: Network {
                mode: "nat".to_string(),
                hostname,
                forwards: Vec::new(),
            },
            integration: Integration {
                tools: if fast { "required" } else { "optional" }.to_string(),
                clipboard: true,
                drag_drop: fast,
                dynamic_resolution: true,
                shared_folders: true,
                applications: true,
                windows: true,
            },
            security: Security {
                shared_folder_approval: "required".to_string(),
                guest_command_execution: false,
                signed_agent_updates: true,
            },
            shared_folders: Vec::new(),
        }
    }

    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.schema_version != SCHEMA_VERSION {
            return Err(ConfigError::UnsupportedSchema {
                expected: SCHEMA_VERSION,
                actual: self.schema_version.clone(),
            });
        }
        if self.name.trim().is_empty() {
            return Err(ConfigError::EmptyName);
        }
        validate_boot(self.boot.as_ref())?;
        validate_shared_folders(&self.shared_folders)?;
        Ok(())
    }

    pub fn read(path: &Path) -> Result<Self, ConfigError> {
        let manifest = serde_yaml::from_str::<Self>(&fs::read_to_string(path)?)?;
        manifest.validate()?;
        Ok(manifest)
    }

    pub fn write(&self, path: &Path) -> Result<(), ConfigError> {
        self.validate()?;
        fs::write(path, serde_yaml::to_string(self)?)?;
        Ok(())
    }
}

fn stable_share_token(name: &str, host_path: &str) -> String {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in name
        .as_bytes()
        .iter()
        .copied()
        .chain([0])
        .chain(host_path.as_bytes().iter().copied())
    {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    format!("share-{hash:016x}")
}

fn validate_shared_folders(shared_folders: &[SharedFolder]) -> Result<(), ConfigError> {
    let mut names = BTreeSet::new();
    let mut tokens = BTreeSet::new();
    for (index, folder) in shared_folders.iter().enumerate() {
        let name = folder.name.trim();
        if name.is_empty() {
            return Err(ConfigError::EmptySharedFolderField {
                index,
                field: "name",
            });
        }
        if folder.host_path.trim().is_empty() {
            return Err(ConfigError::EmptySharedFolderField {
                index,
                field: "hostPath",
            });
        }
        if !names.insert(name.to_string()) {
            return Err(ConfigError::DuplicateSharedFolderName {
                name: name.to_string(),
            });
        }

        let token = folder.resolved_host_path_token();
        if token.trim().is_empty() {
            return Err(ConfigError::EmptySharedFolderField {
                index,
                field: "hostPathToken",
            });
        }
        if !tokens.insert(token.clone()) {
            return Err(ConfigError::DuplicateSharedFolderToken { token });
        }
    }

    Ok(())
}

pub fn slug(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn validate_boot(boot: Option<&Boot>) -> Result<(), ConfigError> {
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
            return Err(ConfigError::EmptyBootInput { field });
        }
    }

    match mode {
        BootMode::ExistingDisk if boot.installer_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::ExistingDisk if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::ExistingDisk if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::ExistingDisk => Ok(()),
        BootMode::LinuxKernel if boot.kernel_path.is_none() => Err(ConfigError::MissingBootInput {
            mode,
            field: "kernelPath",
        }),
        BootMode::LinuxKernel if boot.installer_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxKernel if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxKernel => Ok(()),
        BootMode::LinuxInstaller if boot.installer_image.is_none() => {
            Err(ConfigError::MissingBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::LinuxInstaller if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::LinuxInstaller if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::LinuxInstaller => Ok(()),
        BootMode::WindowsInstaller if boot.installer_image.is_none() => {
            Err(ConfigError::MissingBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::WindowsInstaller if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::WindowsInstaller if boot.macos_restore_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::WindowsInstaller => Ok(()),
        BootMode::MacosRestore if boot.macos_restore_image.is_none() => {
            Err(ConfigError::MissingBootInput {
                mode,
                field: "macosRestoreImage",
            })
        }
        BootMode::MacosRestore if boot.installer_image.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "installerImage",
            })
        }
        BootMode::MacosRestore if boot.kernel_path.is_some() => {
            Err(ConfigError::UnsupportedBootInput {
                mode,
                field: "kernelPath",
            })
        }
        BootMode::MacosRestore => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_fast_manifest_defaults() {
        let manifest = VmManifest::new(
            "Ubuntu Dev",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        assert_eq!(manifest.network.hostname, "ubuntu-dev.bridgevm.local");
        assert_eq!(manifest.backend.engine, "lightvm");
        assert_eq!(
            manifest.boot.as_ref().map(|boot| boot.mode),
            Some(BootMode::ExistingDisk)
        );
        assert!(manifest.integration.drag_drop);
    }

    #[test]
    fn reads_legacy_manifest_without_boot_section() {
        let yaml = r#"
schemaVersion: bridgevm.io/v1
name: old-fast
mode: fast
guest:
  os: ubuntu
  arch: arm64
backend:
  engine: lightvm
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: metal
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.qcow2
    size: 80GiB
    format: qcow2
    discard: true
network:
  mode: nat
  hostname: old-fast.bridgevm.local
integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
"#;
        let manifest = serde_yaml::from_str::<VmManifest>(yaml).unwrap();

        assert!(manifest.boot.is_none());
        assert!(manifest.shared_folders.is_empty());
        assert!(!manifest.integration.applications);
        assert!(!manifest.integration.windows);
    }

    #[test]
    fn parses_and_validates_shared_folder_entries() {
        let yaml = r#"
schemaVersion: bridgevm.io/v1
name: shares
mode: fast
guest:
  os: ubuntu
  arch: arm64
backend:
  engine: lightvm
resources:
  profile: automatic
  memory: auto
  cpu: auto
display:
  renderer: metal
  framePolicy: adaptive
  retina: true
storage:
  primary:
    path: disks/root.qcow2
    size: 80GiB
    format: qcow2
    discard: true
network:
  mode: nat
  hostname: shares.bridgevm.local
integration:
  tools: required
  clipboard: true
  dragDrop: true
  dynamicResolution: true
  sharedFolders: true
  applications: true
  windows: true
security:
  sharedFolderApproval: required
  guestCommandExecution: false
  signedAgentUpdates: true
sharedFolders:
  - name: workspace
    hostPath: /Users/me/project
    readOnly: false
    hostPathToken: share-token-workspace
  - name: downloads
    hostPath: /Users/me/Downloads
    readOnly: true
"#;
        let manifest = serde_yaml::from_str::<VmManifest>(yaml).unwrap();
        manifest.validate().unwrap();

        assert!(manifest.integration.applications);
        assert!(manifest.integration.windows);
        assert_eq!(manifest.shared_folders.len(), 2);
        assert_eq!(
            manifest.shared_folders[0].resolved_host_path_token(),
            "share-token-workspace"
        );
        assert!(manifest.shared_folders[1]
            .resolved_host_path_token()
            .starts_with("share-"));
    }

    #[test]
    fn rejects_invalid_shared_folder_entries() {
        let mut manifest = VmManifest::new(
            "shares",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.shared_folders = vec![SharedFolder {
            name: "".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: None,
        }];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::EmptySharedFolderField { field: "name", .. })
        ));

        manifest.shared_folders = vec![SharedFolder {
            name: "   ".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: None,
        }];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::EmptySharedFolderField { field: "name", .. })
        ));

        manifest.shared_folders = vec![SharedFolder {
            name: "workspace".to_string(),
            host_path: "".to_string(),
            read_only: false,
            host_path_token: None,
        }];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::EmptySharedFolderField {
                field: "hostPath",
                ..
            })
        ));

        manifest.shared_folders = vec![SharedFolder {
            name: "workspace".to_string(),
            host_path: "   ".to_string(),
            read_only: false,
            host_path_token: None,
        }];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::EmptySharedFolderField {
                field: "hostPath",
                ..
            })
        ));

        manifest.shared_folders = vec![SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: Some("".to_string()),
        }];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::EmptySharedFolderField {
                field: "hostPathToken",
                ..
            })
        ));

        manifest.shared_folders = vec![SharedFolder {
            name: "workspace".to_string(),
            host_path: "/Users/me/project".to_string(),
            read_only: false,
            host_path_token: Some("   ".to_string()),
        }];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::EmptySharedFolderField {
                field: "hostPathToken",
                ..
            })
        ));

        manifest.shared_folders = vec![
            SharedFolder {
                name: "workspace".to_string(),
                host_path: "/Users/me/project".to_string(),
                read_only: false,
                host_path_token: Some("share-token".to_string()),
            },
            SharedFolder {
                name: "workspace".to_string(),
                host_path: "/Users/me/other".to_string(),
                read_only: false,
                host_path_token: Some("share-token-2".to_string()),
            },
        ];
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::DuplicateSharedFolderName { .. })
        ));

        manifest.shared_folders[1].name = "other".to_string();
        manifest.shared_folders[1].host_path_token = Some("share-token".to_string());
        assert!(matches!(
            manifest.validate(),
            Err(ConfigError::DuplicateSharedFolderToken { .. })
        ));
    }

    #[test]
    fn rejects_boot_mode_missing_required_media() {
        let mut manifest = VmManifest::new(
            "Ubuntu Installer",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(Boot {
            mode: BootMode::LinuxInstaller,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let error = manifest.validate().unwrap_err();

        assert!(matches!(
            error,
            ConfigError::MissingBootInput {
                mode: BootMode::LinuxInstaller,
                field: "installerImage"
            }
        ));
    }

    #[test]
    fn windows_installer_requires_installer_image() {
        let mut manifest = VmManifest::new(
            "Windows 11 Arm",
            VmMode::Compatibility,
            Guest {
                os: "windows".to_string(),
                version: Some("11".to_string()),
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(Boot {
            mode: BootMode::WindowsInstaller,
            installer_image: None,
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        assert!(matches!(
            manifest.validate().unwrap_err(),
            ConfigError::MissingBootInput {
                mode: BootMode::WindowsInstaller,
                field: "installerImage"
            }
        ));

        manifest.boot = Some(Boot {
            mode: BootMode::WindowsInstaller,
            installer_image: Some("media/win11.iso".to_string()),
            kernel_path: None,
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn rejects_empty_boot_media_path() {
        let mut manifest = VmManifest::new(
            "Ubuntu Kernel",
            VmMode::Fast,
            Guest {
                os: "ubuntu".to_string(),
                version: None,
                arch: "arm64".to_string(),
            },
            "80GiB",
        );
        manifest.boot = Some(Boot {
            mode: BootMode::LinuxKernel,
            installer_image: None,
            kernel_path: Some(" ".to_string()),
            initrd_path: None,
            kernel_command_line: None,
            macos_restore_image: None,
        });

        let error = manifest.validate().unwrap_err();

        assert!(matches!(
            error,
            ConfigError::EmptyBootInput {
                field: "kernelPath"
            }
        ));
    }
}
